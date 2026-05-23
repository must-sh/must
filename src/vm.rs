use std::{collections::HashMap, io::stdin};

use salsa::Id;

use crate::bytecode::{Func, Inst, Terminator};

pub struct VM<'a> {
    funcs: &'a HashMap<String, Func>,
    stack: Vec<Value>,
}

#[derive(Debug, Clone, Copy)]
pub enum Value {
    Int(i32),
    Bool(bool),
}

impl<'a> VM<'a> {
    pub fn new(funcs: &'a HashMap<String, Func>) -> Self {
        let stack = vec![];
        Self { funcs, stack }
    }

    pub fn eval_func(&mut self, name: &str, n: usize) -> Option<Value> {
        let f = match self.funcs.get(name) {
            Some(f) => f,
            None => return self.call_intrinsic(name),
        };

        let mut variables = vec![Value::Int(0); f.variables];

        let mut current_block = 0;
        loop {
            for inst in &f.blocks[current_block].insts {
                match inst {
                    Inst::Push(n) => self.stack.push(Value::Int(*n)),
                    Inst::Binop(op) => {
                        use crate::common::Op::*;
                        use Value::*;
                        let res = match (op, self.stack.pop()?, self.stack.pop()?) {
                            (Add, Int(y), Int(x)) => Int(x + y),
                            (Sub, Int(y), Int(x)) => Int(x - y),
                            (Mul, Int(y), Int(x)) => Int(x * y),
                            (Div, Int(y), Int(x)) => Int(x / y),
                            (Eq, Int(y), Int(x)) => Bool(x == y),
                            (Lt, Int(y), Int(x)) => Bool(x < y),
                            _ => panic!(),
                        };
                        self.stack.push(res)
                    }
                    Inst::Set(n) => {
                        let val = self.stack.pop()?;
                        variables[*n] = val;
                    }
                    Inst::Get(n) => {
                        let val = variables[*n];
                        self.stack.push(val);
                    }
                    Inst::Call(name, n) => {
                        let ret = self.eval_func(name, *n)?;
                        self.stack.push(ret)
                    }
                }
            }

            match &f.blocks[current_block].terminator {
                Terminator::Jmp(id) => current_block = *id,
                Terminator::Br(th, el) => {
                    if let Value::Bool(cond) = self.stack.pop()? {
                        current_block = if cond { *th } else { *el };
                    }
                }
                Terminator::Ret => return self.stack.pop(),
            }
        }
    }

    fn call_intrinsic(&mut self, name: &str) -> Option<Value> {
        match name {
            "read" => {
                let mut buf = String::new();
                stdin().read_line(&mut buf).expect("failed to get input");
                let val = buf
                    .trim()
                    .parse::<i32>()
                    .expect("this is not a valid integer");
                Some(Value::Int(val))
            }
            "print" => {
                let val = self.stack.pop()?;
                println!("{val:?}");
                Some(Value::Int(0))
            }
            _ => panic!("unknown intrinsic: {name}"),
        }
    }
}
