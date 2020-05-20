use crate::shadelang::ast::*;

pub mod constant_folding;

pub static mut COUNTER: i32 = 0;

pub fn compile(mut ast: Program) -> VMProgram {
    constant_folding::fold(&mut ast).unwrap();

    codegen(ast)
}

pub fn codegen(ast: Program) -> VMProgram {
    let mut program = VMProgram::new();
    let mut static_section = 0;

    for i in ast.in_parameters.iter() {
        program.data.global_symbols.insert(
            i.ident.item.clone(),
            SymbolMeta {
                offset: static_section,
                type_kind: *i.type_kind,
                is_static: true,
            },
        );

        static_section += 4;
    }

    for o in ast.out_parameters.iter() {
        program.data.global_symbols.insert(
            o.ident.item.clone(),
            SymbolMeta {
                offset: static_section,
                type_kind: *o.type_kind,
                is_static: true,
            },
        );

        static_section += 4;
    }

    for f in ast.functions.iter() {
        let mut func_meta = FuncMeta::new(program.code.len());
        let mut has_return = false;
        let mut stack_offset = 0;

        for s in f.statements.iter() {
            match s {
                Statement::Assignment(i, expr) => {
                    generate_expr(&mut program, &ast, &func_meta, &expr);

                    if let Some(o) = program.data.global_symbols.get(&i.item) {
                        program
                            .code
                            .push(MemoryCell::with_data(OpCode::Mov4Global, o.offset as u16));
                    } else {
                        func_meta.symbols.insert(
                            i.item.clone(),
                            SymbolMeta {
                                offset: stack_offset,
                                is_static: false,
                                type_kind: TypeKind::F32,
                            },
                        );
                        stack_offset += 4;
                    }
                }
                Statement::Return(expr) => {
                    generate_expr(&mut program, &ast, &func_meta, &expr);
                    program
                        .code
                        .push(MemoryCell::with_data(OpCode::Ret, stack_offset as u16));
                    has_return = true;
                }
            }
        }
        if !has_return {
            program.code.push(MemoryCell::plain_inst(OpCode::Void));
            program
                .code
                .push(MemoryCell::with_data(OpCode::Ret, stack_offset as u16));
        }

        program
            .data
            .functions
            .insert(f.ident.item.clone(), func_meta);
    }

    program.data.static_section_size = static_section;
    program.data.min_stack_size = static_section + 1024;
    program
}

pub fn generate_expr(program: &mut VMProgram, ast: &Program, fnc: &FuncMeta, expr: &Expr) {
    match expr {
        Expr::BinaryOp(op, e1, e2) => {
            generate_expr(program, ast, fnc, e1);
            generate_expr(program, ast, fnc, e2);

            let inst = match op {
                BinaryOperator::Add => OpCode::AddF32,
                BinaryOperator::Sub => OpCode::SubF32,
                BinaryOperator::Mul => OpCode::MulF32,
                BinaryOperator::Div => OpCode::DivF32,
            };

            program.code.push(MemoryCell::plain_inst(inst));
        }
        Expr::FuncCall(id, _args) => {
            let offset = program.data.functions.get(&id.item).unwrap().address;

            program
                .code
                .push(MemoryCell::with_data(OpCode::Call, offset as u16));
        }
        Expr::Literal(l) => match l.item {
            Literal::DecimalLiteral(f) => {
                program.code.push(MemoryCell::plain_inst(OpCode::ConstF32));
                program
                    .code
                    .push(MemoryCell::raw(unsafe { std::mem::transmute(f as f32) }));
            }
            _ => unimplemented!(),
        },
        Expr::Symbol(s) => {
            if let Some(symbol) = fnc.symbols.get(&s.ident) {
                program
                    .code
                    .push(MemoryCell::with_data(OpCode::Load4, symbol.offset as u16));
            } else if let Some(symbol) = program.data.global_symbols.get(&s.ident) {
                program.code.push(MemoryCell::with_data(
                    OpCode::Load4Global,
                    symbol.offset as u16,
                ));
            } else {
                panic!("Unknown symbol: {:?}", s);
            }
        }
        Expr::UnaryOp(op, rhs) => match op {
            UnaryOperator::Sub => {
                generate_expr(program, ast, fnc, rhs);
                program.code.push(MemoryCell::plain_inst(OpCode::ConstF32));
                program
                    .code
                    .push(MemoryCell::raw(unsafe { std::mem::transmute(-1.0 as f32) }));

                program.code.push(MemoryCell::plain_inst(OpCode::MulF32));
            }
            _ => unimplemented!(),
        },
        _ => {
            dbg!(expr);
            unimplemented!();
        }
    }
}

#[repr(u16)]
#[derive(Clone, Copy, Debug)]
pub enum OpCode {
    AddI32,
    SubI32,
    MulI32,
    DivI32,

    AddF32,
    SubF32,
    MulF32,
    DivF32,

    ConstF32,
    Void,
    Mov4,
    Load4,
    Mov4Global,
    Load4Global,

    Ret,
    Call,
    Jmp,
    JmpIf,
}

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct SymbolMeta {
    pub offset: usize,
    pub is_static: bool,
    pub type_kind: TypeKind,
}

#[derive(Debug, Clone)]
pub struct FuncMeta {
    pub address: usize,
    pub symbols: HashMap<String, SymbolMeta>,
}

impl FuncMeta {
    pub fn new(address: usize) -> Self {
        FuncMeta {
            address,
            symbols: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProgramData {
    pub functions: HashMap<String, FuncMeta>,
    pub global_symbols: HashMap<String, SymbolMeta>,
    pub min_stack_size: usize,
    pub static_section_size: usize,
}

impl ProgramData {
    pub fn new() -> Self {
        ProgramData {
            functions: HashMap::new(),
            global_symbols: HashMap::new(),
            min_stack_size: 0,
            static_section_size: 0,
        }
    }
}

#[derive(Clone)]
pub struct MemoryCell {
    pub data: u32,
}

impl std::fmt::Debug for MemoryCell {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (op, ax) = self.get_inst();

        fmt.debug_struct("MemoryCell")
            .field("op", &op)
            .field("ax", &ax)
            .finish()
    }
}

impl MemoryCell {
    pub fn raw(data: u32) -> Self {
        MemoryCell { data }
    }

    pub fn plain_inst(inst: OpCode) -> Self {
        MemoryCell {
            data: inst as u16 as u32,
        }
    }

    pub fn with_data(inst: OpCode, data: u16) -> Self {
        MemoryCell {
            data: (inst as u16 as u32) | ((data as u32) << 16),
        }
    }

    pub fn get_inst(&self) -> (OpCode, u16) {
        (
            unsafe { std::mem::transmute(self.data as u16) },
            (self.data >> 16) as u16,
        )
    }
}

#[derive(Debug, Clone)]
pub struct VMProgram {
    pub code: Vec<MemoryCell>,
    pub data: ProgramData,
}

impl VMProgram {
    pub fn new() -> Self {
        VMProgram {
            code: Vec::new(),
            data: ProgramData::new(),
        }
    }
}
