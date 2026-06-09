use std::collections::HashMap;

use cranelift_codegen::{
    ir::{
        self, AbiParam, ArgumentPurpose, InstBuilder, MemFlags, Signature, StackSlotData,
        condcodes::IntCC, types,
    },
    isa,
    settings::{self, Configurable},
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Linkage, Module};
use cranelift_object::{ObjectModule, ObjectProduct};

use crate::bytecode;

impl bytecode::Type {
    pub fn as_cranelift_tp(self) -> ir::Type {
        match self {
            bytecode::Type::Int64 => types::I64,
            bytecode::Type::Bool => types::I8,
            bytecode::Type::Ptr => types::I64,
        }
    }
}

pub fn compile(prog: bytecode::Prog, print: bool) -> ObjectProduct {
    let mut settings_builder = settings::builder();
    settings_builder.set("opt_level", "speed").unwrap();
    let flags = settings::Flags::new(settings_builder);
    let isa = isa::lookup_by_name("x86_64-linux-elf")
        .unwrap()
        .finish(flags)
        .unwrap();

    let module_builder = cranelift_object::ObjectBuilder::new(
        isa,
        "output",
        cranelift_module::default_libcall_names(),
    )
    .unwrap();

    let module = ObjectModule::new(module_builder);

    let mut lowerer = Lowerer::new(module, print);

    for (name, f) in &prog.funcs {
        lowerer.declare_fn(name.clone(), &f.sig, Linkage::Export);
    }

    for (name, sig) in &prog.externs {
        lowerer.declare_fn(name.clone(), &sig, Linkage::Import);
    }

    for (name, f) in prog.funcs {
        lowerer.define_fn(name, f);
    }

    let obj = lowerer.finish();
    obj
}

struct Lowerer {
    m: ObjectModule,
    fn_map: HashMap<String, FuncId>,
    fn_sigs: HashMap<FuncId, Signature>,
    print: bool,
}

impl Lowerer {
    fn new(m: ObjectModule, print: bool) -> Self {
        Self {
            m,
            fn_map: HashMap::new(),
            fn_sigs: HashMap::new(),
            print,
        }
    }

    fn finish(self) -> ObjectProduct {
        self.m.finish()
    }

    fn declare_fn(&mut self, name: String, f: &bytecode::FuncSig, linkage: Linkage) {
        let mut sig = self.m.make_signature();
        for ret in &f.rets {
            match ret.abi() {
                bytecode::Abi::Scalar(tp) => sig.returns.push(AbiParam::new(tp.as_cranelift_tp())),
                bytecode::Abi::ScalarPair(tp1, tp2) => {
                    sig.returns.push(AbiParam::new(tp1.as_cranelift_tp()));
                    sig.returns.push(AbiParam::new(tp2.as_cranelift_tp()))
                }
                bytecode::Abi::Struct => sig.params.push(AbiParam::special(
                    bytecode::Type::Ptr.as_cranelift_tp(),
                    ArgumentPurpose::StructReturn,
                )),
                bytecode::Abi::Unit => (),
            };
        }
        for arg in &f.args {
            match arg.abi() {
                bytecode::Abi::Scalar(tp) => sig.params.push(AbiParam::new(tp.as_cranelift_tp())),
                bytecode::Abi::ScalarPair(tp1, tp2) => {
                    sig.params.push(AbiParam::new(tp1.as_cranelift_tp()));
                    sig.params.push(AbiParam::new(tp2.as_cranelift_tp()))
                }
                bytecode::Abi::Struct => sig
                    .params
                    .push(AbiParam::new(bytecode::Type::Ptr.as_cranelift_tp())),
                bytecode::Abi::Unit => (),
            };
        }
        let id = self.m.declare_function(&name, linkage, &sig).unwrap();
        self.fn_map.insert(name, id);
        self.fn_sigs.insert(id, sig);
    }

    fn define_fn(&mut self, name: String, f: bytecode::Func) {
        let func = self.get_func_id(&name);

        let mut ctx = self.m.make_context();
        let mut fn_ctx = FunctionBuilderContext::new();

        ctx.func.signature = self
            .m
            .declarations()
            .get_function_decl(func)
            .signature
            .clone();

        let mut b = FunctionBuilder::new(&mut ctx.func, &mut fn_ctx);

        let blocks: HashMap<_, _> = (0..f.blocks.len())
            .map(|id| (id, b.create_block()))
            .collect();

        let entry_block = *blocks.get(&0).unwrap();
        b.append_block_params_for_function_params(entry_block);

        let mut variable_map = HashMap::new();

        for (id, size) in f.variables.iter().enumerate() {
            let ss = b.create_sized_stack_slot(StackSlotData::new(
                cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                *size,
                8,
            ));
            variable_map.insert(id, ss);
        }

        let mut stack = vec![];
        let fn_args = b.block_params(entry_block);

        for arg in fn_args {
            stack.push(*arg);
        }

        for (id, blk) in f.blocks.into_iter().enumerate() {
            let block = *blocks.get(&id).unwrap();
            b.switch_to_block(block);
            for inst in blk.insts {
                match inst {
                    bytecode::Inst::PushInt(n) => {
                        let v = b.ins().iconst(types::I64, n);
                        stack.push(v);
                    }
                    bytecode::Inst::PushBool(n) => {
                        let v = b.ins().iconst(types::I8, n as i64);
                        stack.push(v);
                    }
                    bytecode::Inst::Binop(binop) => {
                        use crate::common::Binop::*;
                        let v2 = stack.pop().unwrap();
                        let v1 = stack.pop().unwrap();
                        let val = match binop {
                            Add => b.ins().iadd(v1, v2),
                            Sub => b.ins().isub(v1, v2),
                            Mul => b.ins().imul(v1, v2),
                            Div => b.ins().sdiv(v1, v2),
                            Mod => b.ins().srem(v1, v2),
                            Eq => b.ins().icmp(IntCC::Equal, v1, v2),
                            Lt => b.ins().icmp(IntCC::SignedLessThan, v1, v2),
                            Gt => b.ins().icmp(IntCC::SignedGreaterThan, v1, v2),
                            Le => b.ins().icmp(IntCC::SignedLessThanOrEqual, v1, v2),
                            Ge => b.ins().icmp(IntCC::SignedGreaterThanOrEqual, v1, v2),
                            NEq => b.ins().icmp(IntCC::NotEqual, v1, v2),
                            And => b.ins().band(v1, v2),
                            Or => b.ins().bor(v1, v2),
                        };
                        stack.push(val)
                    }
                    bytecode::Inst::Unop(unop) => {
                        use crate::common::Unop::*;
                        let v1 = stack.pop().unwrap();
                        let val = match unop {
                            Not => b.ins().bnot(v1),
                            Neg => b.ins().ineg(v1),
                        };
                        stack.push(val)
                    }
                    bytecode::Inst::Set { id, offset } => {
                        let ss = variable_map.get(&id).unwrap();
                        let val = stack.pop().unwrap();
                        b.ins().stack_store(val, *ss, offset as i32);
                    }
                    bytecode::Inst::Get { id, offset, tp } => {
                        let ss = variable_map.get(&id).unwrap();
                        let val = b.ins().stack_load(tp.as_cranelift_tp(), *ss, offset as i32);
                        stack.push(val);
                    }
                    bytecode::Inst::LocalAddr { id, offset } => {
                        let ss = variable_map.get(&id).unwrap();
                        let v = b.ins().stack_addr(types::I64, *ss, offset as i32);
                        stack.push(v);
                    }
                    bytecode::Inst::Load { offset, tp } => {
                        let ptr = stack.pop().unwrap();
                        let val =
                            b.ins()
                                .load(tp.as_cranelift_tp(), MemFlags::new(), ptr, offset as i32);
                        stack.push(val);
                    }
                    bytecode::Inst::Store { offset } => {
                        let ptr = stack.pop().unwrap();
                        let val = stack.pop().unwrap();
                        b.ins().store(MemFlags::new(), val, ptr, offset as i32);
                    }
                    bytecode::Inst::CapOffset => {
                        let ptr = stack.pop().unwrap();
                        let val = stack.pop().unwrap();
                        let res = b.ins().iadd(ptr, val);
                        stack.push(res);
                    }
                    bytecode::Inst::Drop => {
                        stack.pop().unwrap();
                    }
                    bytecode::Inst::Call(name) => {
                        let func_id = self.get_func_id(&name);
                        let f = self.m.declare_func_in_func(func_id, &mut b.func);
                        let n = self.get_func_sig(func_id).params.len();
                        let mut args = vec![];
                        for _ in 0..n {
                            let val = stack.pop().unwrap();
                            args.push(val);
                        }
                        args.reverse();
                        let res = b.ins().call(f, &args);
                        for v in b.inst_results(res) {
                            stack.push(*v)
                        }
                    }
                    bytecode::Inst::MemCopy { size } => {
                        let src = stack.pop().unwrap();
                        let dest = stack.pop().unwrap();
                        let config = self.m.target_config();
                        b.emit_small_memory_copy(
                            config,
                            dest,
                            src,
                            size as u64,
                            8,
                            8,
                            true,
                            MemFlags::new(),
                        );
                    }
                    bytecode::Inst::Dup => {
                        let v = stack.last().unwrap();
                        stack.push(*v);
                    }
                };
            }

            match blk.terminator {
                bytecode::Terminator::Jmp(n) => {
                    let blk = *blocks.get(&n).unwrap();
                    b.ins().jump(blk, &[]);
                }
                bytecode::Terminator::Br(th, el) => {
                    let blk_th = *blocks.get(&th).unwrap();
                    let blk_el = *blocks.get(&el).unwrap();
                    let cond = stack.pop().unwrap();
                    b.ins().brif(cond, blk_th, &[], blk_el, &[]);
                }
                bytecode::Terminator::Ret => {
                    let n = self.get_func_sig(func).returns.len();
                    let mut rets = vec![];
                    for _ in 0..n {
                        let val = stack.pop().unwrap();
                        rets.push(val);
                    }
                    rets.reverse(); // MAYBE?
                    b.ins().return_(&rets);
                }
            };
        }
        b.seal_all_blocks();
        b.finalize();

        self.m.define_function(func, &mut ctx).unwrap();

        if self.print {
            println!("{:?}", name);
            println!("{}", ctx.func);
        }
    }

    fn get_func_id(&self, name: &str) -> FuncId {
        *self.fn_map.get(name).unwrap()
    }

    fn get_func_sig(&self, id: FuncId) -> &Signature {
        self.fn_sigs.get(&id).unwrap()
    }
}
