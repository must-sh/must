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
    Int(i64),
    Bool(bool),
    Ref(usize),
}

impl Value {
    fn as_bytes(self) -> Vec<u8> {
        match self {
            Value::Int(n) => n.to_le_bytes().to_vec(),
            Value::Bool(b) => vec![b as u8],
            Value::Ref(n) => n.to_le_bytes().to_vec(),
        }
    }

    fn from_bytes(bytes: &[u8], tp: &bytecode::Type) -> Self {
        let (b, _) = bytes.split_at(tp.size() as usize);
        match tp {
            bytecode::Type::Int64 => Value::Int(i64::from_le_bytes(b.try_into().unwrap())),
            bytecode::Type::Bool => Value::Bool(b[0] != 0),
            bytecode::Type::Ptr => Value::Ref(usize::from_le_bytes(b.try_into().unwrap())),
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
            hp: 4096,
        }
    }

    pub fn eval_func(&mut self, name: &str) -> Option<()> {
        let f = match self.funcs.get(name) {
            Some(f) => f,
            None => return self.call_intrinsic(name),
        };

        let bp = self.sp;
        self.sp += f.variables.iter().sum::<u32>() as usize;
        if self.sp >= 4096 {
            panic!();
        }

        let local_offsets: Vec<u32> = f
            .variables
            .iter()
            .scan(0, |offset, size| {
                let res = Some(*offset);
                *offset += size;
                res
            })
            .collect();

        let get_local_addr =
            |id: &usize, offset: &u32| bp + local_offsets[*id] as usize + *offset as usize;

        let mut current_block = 0;
        loop {
            for inst in &f.blocks[current_block].insts {
                match inst {
                    Inst::PushInt(n) => self.vstack.push(Value::Int(*n)),
                    Inst::Binop(op) => {
                        use crate::common::Binop::*;
                        use Value::*;
                        let res = match (op, self.vstack.pop().unwrap(), self.vstack.pop().unwrap())
                        {
                            (Add, Int(y), Int(x)) => Int(x + y),
                            (Sub, Int(y), Int(x)) => Int(x - y),
                            (Mul, Int(y), Int(x)) => Int(x * y),
                            (Div, Int(y), Int(x)) => Int(x / y),
                            (Mod, Int(y), Int(x)) => Int(x % y),
                            (Eq, Int(y), Int(x)) => Bool(x == y),
                            (NEq, Int(y), Int(x)) => Bool(x != y),
                            (Lt, Int(y), Int(x)) => Bool(x < y),
                            (Gt, Int(y), Int(x)) => Bool(x > y),
                            (Le, Int(y), Int(x)) => Bool(x <= y),
                            (Ge, Int(y), Int(x)) => Bool(x >= y),
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
                            (Neg, Int(x)) => Int(-x),
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
                        let val = self.vstack.pop().unwrap();
                        if let Value::Ref(mut ptr) = self.vstack.pop().unwrap() {
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
                            (Int(offset), Ref(ptr)) => {
                                self.vstack.push(Ref(ptr + offset as usize));
                            }
                            _ => panic!(),
                        }
                    }
                    Inst::MemCopy { size } => {
                        match (self.vstack.pop().unwrap(), self.vstack.pop().unwrap()) {
                            (Value::Ref(src), Value::Ref(dest)) => {
                                self.memory.copy_within(src..(src + *size as usize), dest);
                            }
                            _ => panic!(),
                        }
                    }
                    Inst::Dup => {
                        let v = self.vstack.last().unwrap();
                        self.vstack.push(*v);
                    }
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
                self.vstack.push(Value::Int(val));
                Some(())
            }
            "must_print" => {
                let val = self.vstack.pop().unwrap();
                println!("{val:?}");
                Some(())
            }
            "must_alloc" => match self.vstack.pop().unwrap() {
                Value::Int(n) => {
                    let ptr = self.hp;
                    self.hp += n as usize;
                    self.vstack.push(Value::Ref(ptr));
                    self.vstack.push(Value::Int(n));
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
