use crate::bytecode::Inst;

pub fn eval(prog: Vec<Inst>) -> Option<i32> {
    let mut stack = vec![];

    for inst in prog {
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
        }
    }

    stack.pop()
}
