use std::{collections::HashMap, io::stdin};

use crate::bytecode::{self, Func, Inst, Terminator};

pub struct VM<'a> {
    funcs: &'a HashMap<String, Func>,
    vstack: Vec<Value>,
    memory: [u8; 1024 * 1024],
    sp: usize,
    hp: usize,
}

#[derive(Debug, Clone, Copy)]
pub enum Value {
    Int64(i64),
    Int128(i128),
    Bool(bool),
    Ref(usize),
}

impl Value {
    fn as_bytes(self) -> Vec<u8> {
        match self {
            Value::Int64(n) => n.to_le_bytes().to_vec(),
            Value::Int128(n) => n.to_le_bytes().to_vec(),
            Value::Bool(b) => vec![b as u8],
            Value::Ref(n) => n.to_le_bytes().to_vec(),
        }
    }

    fn from_bytes(bytes: &[u8], tp: &bytecode::Type) -> Self {
        let (b, _) = bytes.split_at(tp.size() as usize);
        match tp {
            bytecode::Type::Int64 => Value::Int64(i64::from_le_bytes(b.try_into().unwrap())),
            bytecode::Type::Bool => Value::Bool(b[0] != 0),
            // bytecode::Type::Ptr => Value::Ref(usize::from_le_bytes(b.try_into().unwrap())),
            bytecode::Type::Int128 => Value::Int128(i128::from_le_bytes(b.try_into().unwrap())),
            bytecode::Type::Int32 => todo!(),
            bytecode::Type::Int16 => todo!(),
            bytecode::Type::Int8 => todo!(),
            bytecode::Type::UInt128 => todo!(),
            bytecode::Type::UInt64 => todo!(),
            bytecode::Type::UInt32 => todo!(),
            bytecode::Type::UInt16 => todo!(),
            bytecode::Type::UInt8 => todo!(),
            bytecode::Type::Float16 => todo!(),
            bytecode::Type::Float32 => todo!(),
            bytecode::Type::Float64 => todo!(),
            bytecode::Type::Float128 => todo!(),
        }
    }
}

impl<'a> VM<'a> {
    pub fn new(funcs: &'a HashMap<String, Func>) -> Self {
        let vstack = vec![];
        Self {
            funcs,
            vstack,
            memory: [0; 1024 * 1024],
            sp: 0,
            hp: 512 * 1024,
        }
    }

    pub fn eval_func(&mut self, name: &str) -> Option<()> {
        let f = match self.funcs.get(name) {
            Some(f) => f,
            None => return self.call_intrinsic(name),
        };

        let bp = self.sp;
        self.sp += f.variables.iter().map(|lt| lt.size()).sum::<usize>();
        if self.sp >= 512 * 1024 {
            panic!();
        }

        let local_offsets: Vec<u32> = f
            .variables
            .iter()
            .scan(0, |offset, lt| {
                let res = Some(*offset);
                *offset += lt.size() as u32;
                res
            })
            .collect();

        let get_local_addr =
            |id: &usize, offset: &u32| bp + local_offsets[*id] as usize + *offset as usize;

        let mut current_block = 0;
        loop {
            for inst in &f.blocks[current_block].insts {
                match inst {
                    Inst::PushInt(n) => self.vstack.push(Value::Int64(*n)),
                    Inst::Binop(op) => {
                        use crate::common::Binop::*;
                        use Value::*;
                        let res = match (op, self.vstack.pop().unwrap(), self.vstack.pop().unwrap())
                        {
                            (Add, Int64(y), Int64(x)) => Int64(x + y),
                            (Sub, Int64(y), Int64(x)) => Int64(x - y),
                            (Mul, Int64(y), Int64(x)) => Int64(x * y),
                            (Div, Int64(y), Int64(x)) => Int64(x / y),
                            (Mod, Int64(y), Int64(x)) => Int64(x % y),
                            (Eq, Int64(y), Int64(x)) => Bool(x == y),
                            (NEq, Int64(y), Int64(x)) => Bool(x != y),
                            (Lt, Int64(y), Int64(x)) => Bool(x < y),
                            (Gt, Int64(y), Int64(x)) => Bool(x > y),
                            (Le, Int64(y), Int64(x)) => Bool(x <= y),
                            (Ge, Int64(y), Int64(x)) => Bool(x >= y),
                            (And, Bool(y), Bool(x)) => Bool(x && y),
                            (Or, Bool(y), Bool(x)) => Bool(x || y),
                            x => {
                                panic!("{:#?}\n stack:\n{:#?}", x, &self.vstack[..])
                            }
                        };
                        self.vstack.push(res)
                    }
                    Inst::Unop(op) => {
                        use crate::common::Unop::*;
                        use Value::*;
                        let res = match (op, self.vstack.pop().unwrap()) {
                            (Neg, Int64(x)) => Int64(-x),
                            (Not, Bool(x)) => Bool(!x),
                            x => panic!("{:#?}\n stack:\n{:#?}", x, &self.memory[0..self.sp]),
                        };
                        self.vstack.push(res)
                    }
                    Inst::Set { id, offset } => {
                        let mut ptr = get_local_addr(id, offset);
                        let val = self.vstack.pop().unwrap();
                        for b in val.as_bytes() {
                            self.memory[ptr] = b;
                            ptr += 1;
                        }
                    }
                    Inst::Get { id, offset, tp } => {
                        let ptr = get_local_addr(id, offset);
                        let val = Value::from_bytes(&self.memory[ptr..], tp);
                        self.vstack.push(val);
                    }
                    Inst::Call(name) => self.eval_func(name).unwrap(),
                    Inst::LocalAddr { id, offset } => {
                        let ptr = Value::Ref(get_local_addr(id, offset));
                        self.vstack.push(ptr)
                    }
                    Inst::Load { offset, tp } => {
                        if let Value::Ref(ptr) = self.vstack.pop().unwrap() {
                            let val =
                                Value::from_bytes(&self.memory[(ptr + *offset as usize)..], tp);
                            self.vstack.push(val);
                        }
                    }
                    Inst::Store { offset } => {
                        if let Value::Ref(mut ptr) = self.vstack.pop().unwrap() {
                            let val = self.vstack.pop().unwrap();
                            for b in val.as_bytes() {
                                self.memory[ptr + *offset as usize] = b;
                                ptr += 1;
                            }
                        }
                    }
                    Inst::Drop => {
                        self.vstack.pop().unwrap();
                    }
                    Inst::PushBool(b) => self.vstack.push(Value::Bool(*b)),
                    Inst::CapOffset => {
                        use Value::*;
                        match (self.vstack.pop().unwrap(), self.vstack.pop().unwrap()) {
                            (Int64(offset), Ref(ptr)) => {
                                self.vstack.push(Ref(ptr + offset as usize));
                            }
                            (x, y) => panic!("in {}, {:?} {:?}", name, x, y),
                        }
                    }
                    Inst::MemCopy { size, align: _ } => {
                        match (self.vstack.pop().unwrap(), self.vstack.pop().unwrap()) {
                            (Value::Ref(src), Value::Ref(dest)) => {
                                self.memory.copy_within(src..(src + *size), dest);
                            }
                            _ => panic!(),
                        }
                    }
                    Inst::Dup => {
                        let v = self.vstack.last().unwrap();
                        self.vstack.push(*v);
                    }
                    Inst::CallDynamic(_) => todo!(),
                    Inst::FnAddr(_) => todo!(),
                }
            }

            match &f.blocks[current_block].terminator {
                Terminator::Jmp(id) => current_block = *id,
                Terminator::Br(th, el) => {
                    if let Value::Bool(cond) = self.vstack.pop().unwrap() {
                        current_block = if cond { *th } else { *el };
                    }
                }
                Terminator::Ret => {
                    self.sp = bp;
                    return Some(());
                }
            }
        }
    }

    fn call_intrinsic(&mut self, name: &str) -> Option<()> {
        match name {
            "must_read" => {
                let mut buf = String::new();
                stdin().read_line(&mut buf).expect("failed to get input");
                let val = buf
                    .trim()
                    .parse::<i64>()
                    .expect("this is not a valid integer");
                self.vstack.push(Value::Int64(val));
                Some(())
            }
            "must_print" => {
                let val = self.vstack.pop().unwrap();
                println!("{val:?}");
                Some(())
            }
            "must_alloc" => match self.vstack.pop().unwrap() {
                Value::Int64(n) => {
                    let ptr = self.hp;
                    self.hp += n as usize;
                    let mut bytes = [0u8; 16];
                    bytes[0..8].copy_from_slice(&ptr.to_le_bytes());
                    bytes[8..16].copy_from_slice(&n.to_le_bytes());
                    self.vstack.push(Value::Int128(i128::from_le_bytes(bytes)));
                    Some(())
                }
                x => panic!("{:?}", x),
            },
            _ => panic!("unknown intrinsic: {name}"),
        }
    }

    pub fn finish(mut self) -> Option<Value> {
        self.vstack.pop()
    }
}
