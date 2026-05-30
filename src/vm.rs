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
    Int(i32),
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
        self.sp += f.variables;
        if self.sp >= 4096 {
            return panic!();
        }

        let mut current_block = 0;
        loop {
            for inst in &f.blocks[current_block].insts {
                match inst {
                    Inst::PushInt(n) => self.vstack.push(Value::Int(*n)),
                    Inst::Binop(op) => {
                        use crate::common::Op::*;
                        use Value::*;
                        let res = match (op, self.vstack.pop()?, self.vstack.pop()?) {
                            (Add, Int(y), Int(x)) => Int(x + y),
                            (Sub, Int(y), Int(x)) => Int(x - y),
                            (Mul, Int(y), Int(x)) => Int(x * y),
                            (Div, Int(y), Int(x)) => Int(x / y),
                            (Eq, Int(y), Int(x)) => Bool(x == y),
                            (Lt, Int(y), Int(x)) => Bool(x < y),
                            x => panic!("{:#?}\n stack:\n{:#?}", x, &self.memory[0..self.sp]),
                        };
                        self.vstack.push(res)
                    }
                    Inst::Set { id, size } => {
                        for i in (0..*size).rev() {
                            let val = self.vstack.pop()?;
                            self.memory[bp + *id + i] = val;
                        }
                    }
                    Inst::Get { id, size } => {
                        for i in 0..*size {
                            let val = self.memory[bp + *id + i];
                            self.vstack.push(val);
                        }
                    }
                    Inst::Call(name) => self.eval_func(name)?,
                    Inst::LocalAddr(n) => {
                        let ptr = Value::Ref(bp + *n);
                        self.vstack.push(ptr)
                    }
                    Inst::Load { offset, size } => {
                        if let Value::Ref(ptr) = self.vstack.pop()? {
                            for i in 0..*size {
                                let val = self.memory[ptr + offset + i];
                                self.vstack.push(val);
                            }
                        }
                    }
                    Inst::Store { offset, size } => {
                        if let Value::Ref(ptr) = self.vstack.pop()? {
                            for i in (0..*size).rev() {
                                let val = self.vstack.pop()?;
                                self.memory[ptr + offset + i] = val;
                            }
                        }
                    }
                    Inst::Drop => {
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
            "read" => {
                let mut buf = String::new();
                stdin().read_line(&mut buf).expect("failed to get input");
                let val = buf
                    .trim()
                    .parse::<i32>()
                    .expect("this is not a valid integer");
                self.vstack.push(Value::Int(val));
                Some(())
            }
            "print" => {
                let val = self.vstack.pop()?;
                println!("{val:?}");
                Some(())
            }
            "malloc" => {
                if let Value::Int(n) = self.vstack.pop()? {
                    let ptr = self.hp;
                    self.hp += n as usize;
                    self.vstack.push(Value::Ref(ptr));
                    self.vstack.push(Value::Int(n));
                    Some(())
                } else {
                    panic!()
                }
            }
            _ => panic!("unknown intrinsic: {name}"),
        }
    }

    pub fn finish(mut self) -> Option<Value> {
        self.vstack.pop()
    }
}
