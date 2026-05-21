use std::{collections::HashMap, io::stdin};

use crate::bytecode::{Func, Inst};

pub struct VM<'a> {
    funcs: &'a HashMap<String, Func>,
    stack: Vec<i32>,
}

impl<'a> VM<'a> {
    pub fn new(funcs: &'a HashMap<String, Func>) -> Self {
        let stack = vec![];
        Self { funcs, stack }
    }

    pub fn eval_func(&mut self, name: &str, n: usize) -> Option<i32> {
        let f = match self.funcs.get(name) {
            Some(f) => f,
            None => return self.call_intrinsic(name),
        };

        let mut variables = vec![0; f.variables];

        for i in (0..n).rev() {
            variables[i] = self.stack.pop()?;
        }

        for inst in &f.insts {
            match inst {
                Inst::Push(n) => self.stack.push(*n),
                Inst::Add => {
                    let y = self.stack.pop()?;
                    let x = self.stack.pop()?;
                    self.stack.push(x + y)
                }
                Inst::Sub => {
                    let y = self.stack.pop()?;
                    let x = self.stack.pop()?;
                    self.stack.push(x - y)
                }
                Inst::Mul => {
                    let y = self.stack.pop()?;
                    let x = self.stack.pop()?;
                    self.stack.push(x * y)
                }
                Inst::Div => {
                    let y = self.stack.pop()?;
                    let x = self.stack.pop()?;
                    self.stack.push(x / y)
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

        self.stack.pop()
    }

    fn call_intrinsic(&mut self, name: &str) -> Option<i32> {
        match name {
            "read" => {
                let mut buf = String::new();
                stdin().read_line(&mut buf).expect("failed to get input");
                let val = buf
                    .trim()
                    .parse::<i32>()
                    .expect("this is not a valid integer");
                Some(val)
            }
            "print" => {
                let val = self.stack.pop()?;
                println!("{val}");
                Some(0)
            }
            _ => panic!("unknown intrinsic: {name}"),
        }
    }
}
