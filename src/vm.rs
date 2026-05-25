use std::{collections::HashMap, io::stdin};

use crate::bytecode::{Func, Inst, Terminator};

pub struct VM<'a> {
    funcs: &'a HashMap<String, Func>,
    operand_stack: Vec<Value>,
    stack: [Value; 4096],
    stack_pointer: usize,
}

#[derive(Debug, Clone, Copy)]
pub enum Value {
    Int(i32),
    Bool(bool),
    Ref(usize),
}

impl<'a> VM<'a> {
    pub fn new(funcs: &'a HashMap<String, Func>) -> Self {
        let operand_stack = vec![];
        Self {
            funcs,
            operand_stack,
            stack: [Value::Int(0); 4096],
            stack_pointer: 0,
        }
    }

    pub fn eval_func(&mut self, name: &str) -> Option<()> {
        let f = match self.funcs.get(name) {
            Some(f) => f,
            None => return self.call_intrinsic(name),
        };

        let bp = self.stack_pointer;
        self.stack_pointer += f.variables;

        let mut current_block = 0;
        loop {
            for inst in &f.blocks[current_block].insts {
                match inst {
                    Inst::Push(n) => self.operand_stack.push(Value::Int(*n)),
                    Inst::Binop(op) => {
                        use crate::common::Op::*;
                        use Value::*;
                        let res = match (op, self.operand_stack.pop()?, self.operand_stack.pop()?) {
                            (Add, Int(y), Int(x)) => Int(x + y),
                            (Sub, Int(y), Int(x)) => Int(x - y),
                            (Mul, Int(y), Int(x)) => Int(x * y),
                            (Div, Int(y), Int(x)) => Int(x / y),
                            (Eq, Int(y), Int(x)) => Bool(x == y),
                            (Lt, Int(y), Int(x)) => Bool(x < y),
                            _ => panic!(),
                        };
                        self.operand_stack.push(res)
                    }
                    Inst::Set(n) => {
                        let val = self.operand_stack.pop()?;
                        self.stack[bp + *n] = val;
                    }
                    Inst::Get(n) => {
                        let val = self.stack[bp + *n];
                        self.operand_stack.push(val);
                    }
                    Inst::Call(name) => {
                        let ret = self.eval_func(name)?;
                        // self.operand_stack.push(ret)
                    }
                    Inst::LocalAddr(n) => {
                        let ptr = Value::Ref(bp + *n);
                        self.operand_stack.push(ptr)
                    }
                    Inst::Load(offset) => {
                        if let Value::Ref(ptr) = self.operand_stack.pop()? {
                            let val = self.stack[ptr + offset];
                            self.operand_stack.push(val);
                        }
                    }
                    Inst::Store(offset) => {
                        let val = self.operand_stack.pop()?;
                        if let Value::Ref(ptr) = self.operand_stack.pop()? {
                            self.stack[ptr + offset] = val;
                        }
                    }
                    Inst::Drop => {
                        self.operand_stack.pop()?;
                    }
                }
            }

            match &f.blocks[current_block].terminator {
                Terminator::Jmp(id) => current_block = *id,
                Terminator::Br(th, el) => {
                    if let Value::Bool(cond) = self.operand_stack.pop()? {
                        current_block = if cond { *th } else { *el };
                    }
                }
                Terminator::Ret => {
                    self.stack_pointer = bp;
                    // return self.operand_stack.pop();
                    return Some(());
                }
            }
        }
    }

    fn call_intrinsic(&mut self, name: &str) -> Option<()> {
        match name {
            "read" => {
                let mut buf = String::new();
                stdin().read_line(&mut buf).expect("failed to get input");
                let val = buf
                    .trim()
                    .parse::<i32>()
                    .expect("this is not a valid integer");
                self.operand_stack.push(Value::Int(val));
                Some(())
            }
            "print" => {
                let val = self.operand_stack.pop()?;
                println!("{val:?}");
                Some(())
            }
            _ => panic!("unknown intrinsic: {name}"),
        }
    }

    pub fn finish(mut self) -> Option<Value> {
        self.operand_stack.pop()
    }
}
