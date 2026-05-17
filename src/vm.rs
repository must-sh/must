use crate::bytecode::{Inst, Prog};

pub fn eval(prog: Prog) -> Option<i32> {
    let mut stack = vec![];

    let mut variables = vec![0; prog.variables];

    for inst in prog.insts {
        match inst {
            Inst::Push(n) => stack.push(n),
            Inst::Add => {
                let y = stack.pop()?;
                let x = stack.pop()?;
                stack.push(x + y)
            }
            Inst::Sub => {
                let y = stack.pop()?;
                let x = stack.pop()?;
                stack.push(x - y)
            }
            Inst::Mul => {
                let y = stack.pop()?;
                let x = stack.pop()?;
                stack.push(x * y)
            }
            Inst::Div => {
                let y = stack.pop()?;
                let x = stack.pop()?;
                stack.push(x / y)
            }
            Inst::Set(n) => {
                let val = stack.pop()?;
                variables[n] = val;
            }
            Inst::Get(n) => {
                let val = variables[n];
                stack.push(val);
            }
        }
    }

    stack.pop()
}
