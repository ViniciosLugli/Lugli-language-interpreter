use crate::{ast::CallArguments, environment::Value};

mod datetime;
mod global;
mod list;
mod number;
mod string;

pub use datetime::DateTimeObject;
pub use global::GlobalObject;
pub use list::ListObject;
pub use number::NumberObject;
pub use string::StringObject;

pub fn arity(name: &str, arity: usize, arguments: &CallArguments, multiples_entry: bool) -> () {
	if multiples_entry {
		if arguments.len() < arity {
			panic!("{} expects {} arguments, but {} were given", name, arity, arguments.len());
		}
	} else {
		if arguments.len() != arity {
			panic!("{} expects exactly {} arguments, but {} were given", name, arity, arguments.len());
		}
	}
}
