use std::{collections::HashMap, io::stdin};

use crate::bytecode::{Func, Inst, Terminator};

pub struct VM<'a> {
    funcs: &'a HashMap<String, Func>,
    vstack: Vec<Value>,
    memory: [Value; 8192],
    sp: usize,
    hp: usize,
}

#[derive(Debug, Clone, Copy)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Ref(usize),
}

impl<'a> VM<'a> {
    pub fn new(funcs: &'a HashMap<String, Func>) -> Self {
        let vstack = vec![];
        Self {
            funcs,
            vstack,
            memory: [Value::Int(0); 8192],
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
            return panic!();
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
            |id: &usize, offset: &i32| bp + local_offsets[*id] as usize + *offset as usize;

        let mut current_block = 0;
        loop {
            for inst in &f.blocks[current_block].insts {
                match inst {
                    Inst::PushInt(n) => self.vstack.push(Value::Int(*n)),
                    Inst::Binop(op) => {
                        use crate::common::Binop::*;
                        use Value::*;
                        let res = match (op, self.vstack.pop()?, self.vstack.pop()?) {
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
                            x => panic!("{:#?}\n stack:\n{:#?}", x, &self.vstack[..]),
                        };
                        self.vstack.push(res)
                    }
                    Inst::Unop(op) => {
                        use crate::common::Unop::*;
                        use Value::*;
                        let res = match (op, self.vstack.pop()?) {
                            (Neg, Int(x)) => Int(-x),
                            (Not, Bool(x)) => Bool(!x),
                            x => panic!("{:#?}\n stack:\n{:#?}", x, &self.memory[0..self.sp]),
                        };
                        self.vstack.push(res)
                    }
                    Inst::Set { id, offset, tp } => {
                        let ptr = get_local_addr(id, offset);
                        let val = self.vstack.pop()?;
                        self.memory[ptr] = val;
                    }
                    Inst::Get { id, offset, tp } => {
                        let ptr = get_local_addr(id, offset);
                        let val = self.memory[ptr];
                        self.vstack.push(val);
                    }
                    Inst::Call(name) => self.eval_func(name)?,
                    Inst::LocalAddr { id, offset } => {
                        let ptr = Value::Ref(get_local_addr(id, offset));
                        self.vstack.push(ptr)
                    }
                    Inst::Load { offset, tp } => {
                        if let Value::Ref(ptr) = self.vstack.pop()? {
                            let val = self.memory[ptr + *offset as usize];
                            self.vstack.push(val);
                        }
                    }
                    Inst::Store { offset, tp } => {
                        if let Value::Ref(ptr) = self.vstack.pop()? {
                            let val = self.vstack.pop()?;
                            self.memory[ptr + *offset as usize] = val;
                        }
                    }
                    Inst::Drop(tp) => {
                        self.vstack.pop()?;
                    }
                    Inst::PushBool(b) => self.vstack.push(Value::Bool(*b)),
                    Inst::CapOffset => {
                        use Value::*;
                        match (self.vstack.pop()?, self.vstack.pop()?) {
                            (Int(offset), Ref(ptr)) => {
                                self.vstack.push(Ref(ptr + offset as usize));
                            }
                            _ => panic!(),
                        }
                    }
                    Inst::MemCopy { size } => match (self.vstack.pop()?, self.vstack.pop()?) {
                        (Value::Ref(src), Value::Ref(dest)) => {
                            self.memory.copy_within(src..(src + *size as usize), dest);
                        }
                        _ => panic!(),
                    },
                    Inst::Dup => {
                        let v = self.vstack.last().unwrap();
                        self.vstack.push(*v);
                    }
                }
            }

            match &f.blocks[current_block].terminator {
                Terminator::Jmp(id) => current_block = *id,
                Terminator::Br(th, el) => {
                    if let Value::Bool(cond) = self.vstack.pop()? {
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
                let val = self.vstack.pop()?;
                println!("{val:?}");
                Some(())
            }
            "must_alloc" => match self.vstack.pop()? {
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
