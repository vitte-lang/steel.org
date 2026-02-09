//! MUF parser module: lexer + AST + parser.

pub mod ast;
pub mod lexer;
pub mod parser;

pub use crate::arscan;
pub use crate::read;
pub use ast::{File, Header, Line, LineToken, Position, Span, Stmt, Value};
pub use parser::{parse_muf, ParseError};
