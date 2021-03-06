use colored::*;
use hashbrown::HashMap;
use std::slice::Iter;
use thiserror::Error;

use crate::{ast::ConditionBlock, ast::*, token::Token};

pub fn parse(tokens: Vec<Token>) -> Result<Program, ParseError> {
	let mut parser = Parser::new(tokens.iter());

	parser.read();
	parser.read();

	let mut program: Program = Vec::new();

	while let Some(statement) = parser.next()? {
		program.push(statement);
	}

	Ok(program)
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
enum Precedence {
	Lowest,
	Statement,
	Assign,
	PlusAssign,
	MinusAssign,
	MultiplyAssign,
	DivideAssign,
	Increment,
	Decrement,
	AndOr,
	LessThanGreaterThan,
	Equals,
	Sum,
	Product,
	Pow,
	Modulo,
	Prefix,
	Call,
}

impl Precedence {
	fn token(token: Token) -> Self {
		match token {
			Token::Asterisk | Token::Slash => Self::Product,
			Token::Plus | Token::Minus => Self::Sum,
			Token::Percent => Self::Modulo,
			Token::Pow => Self::Pow,
			Token::Equals | Token::NotEquals => Self::Equals,
			Token::And | Token::Or | Token::In | Token::NotIn => Self::AndOr,
			Token::Assign => Self::Assign,
			Token::PlusAssign => Self::PlusAssign,
			Token::MinusAssign => Self::MinusAssign,
			Token::MultiplyAssign => Self::MultiplyAssign,
			Token::DivideAssign => Self::DivideAssign,
			Token::Increment => Self::Increment,
			Token::Decrement => Self::Decrement,
			Token::LessThan | Token::GreaterThan | Token::LessThanOrEquals | Token::GreaterThanOrEquals => Self::LessThanGreaterThan,
			Token::LeftParen | Token::Dot | Token::LeftBracket => Self::Call,
			Token::LeftBrace => Self::Statement,
			_ => Self::Lowest,
		}
	}
}

struct Parser<'p> {
	tokens: Iter<'p, Token>,
	current: Token,
	peek: Token,
}

impl<'p> Parser<'p> {
	fn new(tokens: Iter<'p, Token>) -> Self {
		Self { current: Token::Eof, peek: Token::Eof, tokens }
	}

	fn parse_statement(&mut self) -> Result<Statement, ParseError> {
		match self.current {
			Token::Fn => self.parse_fn(true),
			Token::Struct => self.parse_struct(),
			Token::Create => self.parse_create(),
			Token::Const => self.parse_const(),
			Token::If => self.parse_if(),
			Token::For => self.parse_for(),
			Token::While => self.parse_while(),
			Token::Loop => self.parse_loop(),
			Token::Return => self.parse_return(),
			Token::Break => self.parse_break(),
			Token::Continue => self.parse_continue(),
			_ => Ok(Statement::Expression { expression: self.parse_expression(Precedence::Lowest)? }),
		}
	}

	fn parse_for(&mut self) -> Result<Statement, ParseError> {
		self.expect_token_and_read(Token::For)?;

		let (index, value) = if self.current_is(Token::LeftParen) {
			self.expect_token_and_read(Token::LeftParen)?;
			let index = self.expect_identifier_and_read()?;
			self.expect_token_and_read(Token::Comma)?;
			let tuple = (Some(index.into()), self.expect_identifier_and_read()?.into());
			self.expect_token_and_read(Token::RightParen)?;
			tuple
		} else {
			(None, self.expect_identifier_and_read()?.into())
		};

		self.expect_token_and_read(Token::In)?;

		let iterable = self.parse_expression(Precedence::Statement)?;
		let then = self.parse_block()?;

		Ok(Statement::For { index, value, iterable, then })
	}

	fn parse_expression(&mut self, precedence: Precedence) -> Result<Expression, ParseError> {
		let mut left = match self.current.clone() {
			Token::String(s) => {
				self.expect_token_and_read(Token::String("".to_string()))?;
				Expression::String(s.to_string())
			}
			Token::Null => {
				self.expect_token_and_read(Token::Null)?;
				Expression::Null
			}
			Token::Number(n) => {
				self.expect_token_and_read(Token::Number(0.0))?;
				Expression::Number(n)
			}
			Token::True => {
				self.expect_token_and_read(Token::True)?;
				Expression::Bool(true)
			}
			Token::False => {
				self.expect_token_and_read(Token::False)?;
				Expression::Bool(false)
			}
			Token::Identifier(s) => {
				self.expect_identifier_and_read()?;
				Expression::Identifier(s)
			}
			Token::Fn => {
				let (params, body) = match self.parse_fn(false)? {
					Statement::FunctionDeclaration { params, body, .. } => (params, body),
					_ => return Err(ParseError::Unreachable),
				};

				Expression::Closure(params, body)
			}
			t @ Token::Minus | t @ Token::Bang => {
				self.expect_token_and_read(t.clone())?;

				Expression::Prefix(Op::token(t), self.parse_expression(Precedence::Prefix)?.boxed())
			}
			Token::LeftBracket => {
				self.expect_token_and_read(Token::LeftBracket)?;

				let mut items: Vec<Expression> = Vec::new();

				while !self.current_is(Token::RightBracket) {
					items.push(self.parse_expression(Precedence::Lowest)?);

					if self.current_is(Token::Comma) {
						self.expect_token_and_read(Token::Comma)?;
					}
				}

				self.expect_token_and_read(Token::RightBracket)?;

				Expression::List(items)
			}
			_ => return Err(ParseError::UnexpectedToken(self.current.clone())),
		};

		while !self.current_is(Token::Eof) && precedence < Precedence::token(self.current.clone()) {
			if let Some(expression) = self.parse_postfix_expression(left.clone())? {
				left = expression;
			} else if let Some(expression) = self.parse_infix_expression(left.clone())? {
				left = expression
			} else {
				break;
			}
		}

		Ok(left)
	}

	fn parse_arguments(&mut self) -> Result<CallArguments, ParseError> {
		self.expect_token_and_read(Token::LeftParen)?;

		let mut args: CallArguments = CallArguments::new();

		while !self.current_is(Token::RightParen) {
			let expression = self.parse_expression(Precedence::Lowest)?;

			match expression {
				Expression::Assign(param, value) => match *param {
					Expression::Identifier(name) => args.add_argument(Argument::new(Some(name), *value)),
					_ => return Err(ParseError::UnexpectedToken(self.current.clone())),
				},
				_ => args.add_argument(Argument::new(None, expression)),
			};

			if self.current_is(Token::Comma) {
				self.expect_token_and_read(Token::Comma)?;
			}
		}

		self.expect_token_and_read(Token::RightParen)?;

		Ok(args)
	}

	fn parse_postfix_expression(&mut self, left: Expression) -> Result<Option<Expression>, ParseError> {
		Ok(match self.current {
			Token::Dot => {
				self.expect_token_and_read(Token::Dot)?;

				let field: Identifier = self.expect_identifier_and_read()?.into();

				if self.current_is(Token::LeftParen) {
					let args = self.parse_arguments()?;
					Some(Expression::MethodCall(Box::new(left), field, args))
				} else {
					if self.current_is(Token::Assign) {
						self.expect_token_and_read(Token::Assign)?;
						let right = self.parse_expression(Precedence::Lowest)?;
						Some(Expression::SetProperty(Box::new(left), field, Box::new(right)))
					} else {
						Some(Expression::GetProperty(Box::new(left), field))
					}
				}
			}

			Token::LeftBracket => {
				self.expect_token_and_read(Token::LeftBracket)?;

				let index: Option<Box<Expression>> =
					if self.current_is(Token::RightBracket) { None } else { Some(self.parse_expression(Precedence::Lowest)?.boxed()) };

				self.expect_token_and_read(Token::RightBracket)?;

				Some(Expression::Index(left.boxed(), index))
			}
			Token::LeftBrace => {
				self.expect_token_and_read(Token::LeftBrace)?;

				let mut fields: HashMap<Identifier, Expression> = HashMap::new();

				while !self.current_is(Token::RightBrace) {
					let field = self.expect_identifier_and_read()?;

					let value = if self.current_is(Token::Colon) {
						self.expect_token_and_read(Token::Colon)?;
						self.parse_expression(Precedence::Lowest)?
					} else {
						Expression::Identifier(field.clone().into())
					};

					fields.insert(field.into(), value);

					if self.current_is(Token::Comma) {
						self.read();
					}
				}

				self.expect_token_and_read(Token::RightBrace)?;

				Some(Expression::Struct(left.boxed(), fields))
			}
			Token::LeftParen => {
				let args = self.parse_arguments()?;

				Some(Expression::Call(Box::new(left), args))
			}
			_ => None,
		})
	}

	fn parse_infix_expression(&mut self, left: Expression) -> Result<Option<Expression>, ParseError> {
		Ok(match self.current {
			Token::Plus
			| Token::Minus
			| Token::Asterisk
			| Token::Slash
			| Token::Equals
			| Token::NotEquals
			| Token::LessThanOrEquals
			| Token::LessThan
			| Token::GreaterThan
			| Token::GreaterThanOrEquals
			| Token::And
			| Token::Or
			| Token::Pow
			| Token::In
			| Token::NotIn
			| Token::Percent => {
				let token = self.current.clone();

				self.read();

				let right = self.parse_expression(Precedence::token(token.clone()))?;

				Some(Expression::Infix(Box::new(left), Op::token(token), Box::new(right)))
			}
			Token::Assign => {
				self.read();

				let right = self.parse_expression(Precedence::Lowest)?;

				Some(Expression::Assign(Box::new(left), Box::new(right)))
			}

			Token::PlusAssign => {
				self.read();

				let right = self.parse_expression(Precedence::Lowest)?;

				Some(Expression::MathAssign(Box::new(left), Op::Add, Box::new(right)))
			}

			Token::MinusAssign => {
				self.read();

				let right = self.parse_expression(Precedence::Lowest)?;

				Some(Expression::MathAssign(Box::new(left), Op::Subtract, Box::new(right)))
			}

			Token::MultiplyAssign => {
				self.read();

				let right = self.parse_expression(Precedence::Lowest)?;

				Some(Expression::MathAssign(Box::new(left), Op::Multiply, Box::new(right)))
			}

			Token::DivideAssign => {
				self.read();

				let right = self.parse_expression(Precedence::Lowest)?;

				Some(Expression::MathAssign(Box::new(left), Op::Divide, Box::new(right)))
			}

			Token::Increment => {
				self.read();

				Some(Expression::MathAssign(Box::new(left), Op::Add, Box::new(Expression::Number(1.0))))
			}

			Token::Decrement => {
				self.read();

				Some(Expression::MathAssign(Box::new(left), Op::Subtract, Box::new(Expression::Number(1.0))))
			}

			_ => None,
		})
	}

	fn parse_if(&mut self) -> Result<Statement, ParseError> {
		self.expect_token_and_read(Token::If)?;

		let condition_if;

		if self.current_is(Token::LeftParen) {
			self.expect_token_and_read(Token::LeftParen)?;
			condition_if = self.parse_expression(Precedence::Lowest)?;
			self.expect_token_and_read(Token::RightParen)?;
		} else {
			condition_if = self.parse_expression(Precedence::Statement)?;
		}

		let then_if = self.parse_block()?;

		let others_conditions = if self.current_is(Token::ElseIf) {
			let mut others_conditions: Vec<ConditionBlock> = Vec::new();

			while self.current_is(Token::ElseIf) {
				self.expect_token_and_read(Token::ElseIf)?;

				let condition_else_if;

				if self.current_is(Token::LeftParen) {
					self.expect_token_and_read(Token::LeftParen)?;
					condition_else_if = self.parse_expression(Precedence::Lowest)?;
					self.expect_token_and_read(Token::RightParen)?;
				} else {
					condition_else_if = self.parse_expression(Precedence::Statement)?;
				}
				others_conditions.push(ConditionBlock { expression: condition_else_if, then: self.parse_block()? });
			}

			Some(others_conditions)
		} else {
			None
		};

		let otherwise = if self.current_is(Token::Else) {
			self.expect_token_and_read(Token::Else)?;
			Some(self.parse_block()?)
		} else {
			None
		};

		Ok(Statement::If { condition: ConditionBlock { expression: condition_if, then: then_if }, others_conditions, otherwise })
	}

	fn parse_while(&mut self) -> Result<Statement, ParseError> {
		self.expect_token_and_read(Token::While)?;

		let condition = if self.current_is(Token::LeftParen) {
			self.expect_token_and_read(Token::LeftParen)?;
			let condition = self.parse_expression(Precedence::Statement)?;
			self.expect_token_and_read(Token::RightParen)?;
			condition
		} else {
			self.parse_expression(Precedence::Statement)?
		};

		let then = self.parse_block()?;

		Ok(Statement::While { condition: ConditionBlock { expression: condition, then } })
	}

	fn parse_loop(&mut self) -> Result<Statement, ParseError> {
		self.expect_token_and_read(Token::Loop)?;

		let then = self.parse_block()?;

		Ok(Statement::Loop { body: then })
	}

	fn parse_return(&mut self) -> Result<Statement, ParseError> {
		self.expect_token_and_read(Token::Return)?;

		if let Ok(expression) = self.parse_expression(Precedence::Lowest) {
			Ok(Statement::Return { value: expression })
		} else {
			Ok(Statement::Return { value: Expression::Null })
		}
	}

	fn parse_break(&mut self) -> Result<Statement, ParseError> {
		self.expect_token_and_read(Token::Break)?;

		Ok(Statement::Break)
	}

	fn parse_continue(&mut self) -> Result<Statement, ParseError> {
		self.expect_token_and_read(Token::Continue)?;

		Ok(Statement::Continue)
	}

	fn parse_create(&mut self) -> Result<Statement, ParseError> {
		self.expect_token_and_read(Token::Create)?;

		let name: Identifier = self.expect_identifier_and_read()?.into();
		let initial: Option<Expression> = if self.current_is(Token::Assign) {
			self.expect_token_and_read(Token::Assign)?;

			Some(self.parse_expression(Precedence::Lowest)?)
		} else {
			None
		};

		Ok(Statement::CreateDeclaration { name, initial })
	}

	fn parse_const(&mut self) -> Result<Statement, ParseError> {
		self.expect_token_and_read(Token::Const)?;

		let name: Identifier = self.expect_identifier_and_read()?.into();
		self.expect_token_and_read(Token::Assign)?;

		let initial = self.parse_expression(Precedence::Lowest)?;

		Ok(Statement::ConstDeclaration { name, initial })
	}

	fn parse_struct(&mut self) -> Result<Statement, ParseError> {
		self.expect_token_and_read(Token::Struct)?;

		let name: Identifier = self.expect_identifier_and_read()?.into();

		self.expect_token_and_read(Token::LeftBrace)?;

		let mut fields: Vec<Parameter> = Vec::new();

		while !self.current_is(Token::RightBrace) {
			if self.current_is(Token::Fn) {
				let function = self.parse_fn(true)?;
				if let Statement::FunctionDeclaration { name, params, body } = function {
					let closure = Expression::Closure(params.clone(), body);

					fields.push(Parameter { name, initial: Some(closure) });
				} else {
					return Err(ParseError::UnexpectedToken(self.current.clone()));
				}
			} else {
				let field: String = self.expect_identifier_and_read()?.into();

				match self.current.clone() {
					Token::Comma | Token::RightBrace | Token::Fn | Token::Identifier(..) => fields.push(Parameter { name: field, initial: None }),
					Token::Assign => {
						self.expect_token_and_read(Token::Assign)?;

						let initial = self.parse_expression(Precedence::Lowest)?;

						fields.push(Parameter { name: field, initial: Some(initial) });
					}
					_ => unreachable!(),
				}
			}
		}

		self.expect_token_and_read(Token::RightBrace)?;

		Ok(Statement::StructDeclaration { name, fields })
	}

	fn parse_fn(&mut self, with_identifier: bool) -> Result<Statement, ParseError> {
		self.expect_token_and_read(Token::Fn)?;

		let name: Identifier = if with_identifier { self.expect_identifier_and_read()?.into() } else { String::from("<Closure>") };

		self.expect_token_and_read(Token::LeftParen)?;

		let mut params: Vec<Parameter> = Vec::new();

		while !self.current_is(Token::RightParen) {
			if self.current_is(Token::Comma) {
				self.expect_token_and_read(Token::Comma)?;
			}

			let param: String = self.expect_identifier_and_read()?.into();

			if self.current_is(Token::Assign) {
				self.expect_token_and_read(Token::Assign)?;

				let initial = self.parse_expression(Precedence::Lowest)?;

				params.push(Parameter { name: param, initial: Some(initial) });
			} else {
				params.push(Parameter { name: param, initial: None });
			}
		}

		self.expect_token_and_read(Token::RightParen)?;

		let body: Vec<Statement> = self.parse_block()?;

		Ok(Statement::FunctionDeclaration { name, params, body })
	}

	fn parse_block(&mut self) -> Result<Block, ParseError> {
		self.expect_token_and_read(Token::LeftBrace)?;

		let mut block = Vec::new();

		while !self.current_is(Token::RightBrace) {
			block.push(self.parse_statement()?);
		}

		self.expect_token_and_read(Token::RightBrace)?;

		Ok(block)
	}

	fn expect_token(&mut self, token: Token) -> Result<Token, ParseError> {
		if self.current_is(token.clone()) {
			Ok(self.current.clone())
		} else {
			Err(ParseError::UnexpectedTokenExpected(self.current.clone(), token))
		}
	}

	fn expect_token_and_read(&mut self, token: Token) -> Result<Token, ParseError> {
		// TODO: Replace token for optional token
		let result = self.expect_token(token)?;

		self.read();

		Ok(result)
	}

	fn expect_identifier_and_read(&mut self) -> Result<Token, ParseError> {
		self.expect_token_and_read(Token::Identifier("".to_string()))
	}

	fn current_is(&self, token: Token) -> bool {
		std::mem::discriminant(&self.current) == std::mem::discriminant(&token)
	}

	fn read(&mut self) {
		self.current = self.peek.clone();
		self.peek = if let Some(token) = self.tokens.next() { token.clone() } else { Token::Eof };
	}

	fn next(&mut self) -> Result<Option<Statement>, ParseError> {
		if self.current == Token::Eof {
			return Ok(None);
		}

		Ok(Some(self.parse_statement()?))
	}
}

#[derive(Debug, Error)]
pub enum ParseError {
	#[error("Unexpected token `{0:?}`.")]
	UnexpectedToken(Token),

	#[error("Unexpected token `{0:?}`, expected `{1:?}`")]
	UnexpectedTokenExpected(Token, Token),

	#[error("Entered unreachable code.")]
	Unreachable,
}

impl ParseError {
	pub fn print(self) {
		eprintln!("{}", format!("{}", self).red().bold());
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::token;

	fn lex_and_parse(input: &str) -> Program {
		let tokens = token::generate(input);
		parse(tokens).unwrap()
	}

	#[test]
	fn it_can_parse_fn_declarations() {
		assert_eq!(lex_and_parse("fn name() {}"), vec![Statement::FunctionDeclaration { name: String::from("name"), body: vec![], params: vec![] }]);

		assert_eq!(
			lex_and_parse("fn name(person) {}"),
			vec![Statement::FunctionDeclaration {
				name: String::from("name"),
				body: vec![],
				params: vec![Parameter { name: String::from("person") }]
			}]
		);

		assert_eq!(
			lex_and_parse("fn say_hello(name, separator) {}"),
			vec![Statement::FunctionDeclaration {
				name: String::from("say_hello"),
				body: vec![],
				params: vec![Parameter { name: String::from("name") }, Parameter { name: String::from("separator") }]
			}]
		);

		assert_eq!(
			lex_and_parse(
				"
                fn say_hello() {
                    create name = true
                }
            "
			),
			vec![Statement::FunctionDeclaration {
				name: String::from("say_hello"),
				body: vec![Statement::CreateDeclaration { name: String::from("name"), initial: Expression::Bool(true).some() }],
				params: vec![]
			}]
		)
	}

	#[test]
	fn it_can_parse_create_declarations_and_const() {
		assert_eq!(lex_and_parse("create name"), vec![Statement::CreateDeclaration { name: String::from("name"), initial: None }]);

		assert_eq!(
			lex_and_parse("create bool = true"),
			vec![Statement::CreateDeclaration { name: String::from("bool"), initial: Expression::Bool(true).some() }]
		);

		assert_eq!(
			lex_and_parse("const bool = false"),
			vec![Statement::ConstDeclaration { name: String::from("bool"), initial: Expression::Bool(false) }]
		);
	}

	#[test]
	fn it_can_parse_literals() {
		assert_eq!(
			lex_and_parse(r##"123 "testing" true false 123.456"##),
			vec![
				Statement::Expression { expression: Expression::Number(123.0) },
				Statement::Expression { expression: Expression::String("testing".to_owned()) },
				Statement::Expression { expression: Expression::Bool(true) },
				Statement::Expression { expression: Expression::Bool(false) },
				Statement::Expression { expression: Expression::Number(123.456) },
			]
		);
	}

	#[test]
	fn it_can_parse_mathematical_operations() {
		assert_eq!(
			lex_and_parse("1 + 2"),
			vec![Statement::Expression {
				expression: Expression::Infix(Box::new(Expression::Number(1.0)), Op::Add, Box::new(Expression::Number(2.0)))
			}]
		);

		assert_eq!(
			lex_and_parse("1 - 2"),
			vec![Statement::Expression {
				expression: Expression::Infix(Box::new(Expression::Number(1.0)), Op::Subtract, Box::new(Expression::Number(2.0)))
			}]
		);

		assert_eq!(
			lex_and_parse("1 * 2"),
			vec![Statement::Expression {
				expression: Expression::Infix(Box::new(Expression::Number(1.0)), Op::Multiply, Box::new(Expression::Number(2.0)))
			}]
		);

		assert_eq!(
			lex_and_parse("1 / 2"),
			vec![Statement::Expression {
				expression: Expression::Infix(Box::new(Expression::Number(1.0)), Op::Divide, Box::new(Expression::Number(2.0)))
			}]
		);

		assert_eq!(
			lex_and_parse("1 + 2 * 3"),
			vec![Statement::Expression {
				expression: Expression::Infix(
					Box::new(Expression::Number(1.0)),
					Op::Add,
					Box::new(Expression::Infix(Box::new(Expression::Number(2.0)), Op::Multiply, Box::new(Expression::Number(3.0)),))
				)
			}]
		);

		assert_eq!(
			lex_and_parse("1 + 2 * 3 / 3"),
			vec![Statement::Expression {
				expression: Expression::Infix(
					Box::new(Expression::Number(1.0)),
					Op::Add,
					Box::new(Expression::Infix(
						Box::new(Expression::Infix(Box::new(Expression::Number(2.0)), Op::Multiply, Box::new(Expression::Number(3.0)),)),
						Op::Divide,
						Box::new(Expression::Number(3.0)),
					),)
				)
			}]
		);
	}

	#[test]
	fn it_can_parse_call_expressions() {
		assert_eq!(
			lex_and_parse("hello()"),
			vec![Statement::Expression { expression: Expression::Call(Box::new(Expression::Identifier("hello".to_owned())), vec![]) }]
		);

		assert_eq!(
			lex_and_parse("hello(true)"),
			vec![Statement::Expression {
				expression: Expression::Call(Box::new(Expression::Identifier("hello".to_owned())), vec![Expression::Bool(true)])
			}]
		);

		assert_eq!(
			lex_and_parse("hello(true, 1234)"),
			vec![Statement::Expression {
				expression: Expression::Call(
					Box::new(Expression::Identifier("hello".to_owned())),
					vec![Expression::Bool(true), Expression::Number(1234.0)]
				)
			}]
		);
	}

	#[test]
	fn it_can_parse_if_statements() {
		assert_eq!(
			lex_and_parse("if true {}"),
			vec![Statement::If {
				condition: ConditionBlock { expression: Expression::Bool(true), then: vec![] },
				others_conditions: None,
				otherwise: None
			}]
		);

		assert_eq!(
			lex_and_parse(
				"if true {
					create number = 1
				}"
			),
			vec![Statement::If {
				condition: ConditionBlock {
					expression: Expression::Bool(true),
					then: vec![Statement::CreateDeclaration { name: String::from("number"), initial: Some(Expression::Number(1.0)) }]
				},
				others_conditions: None,
				otherwise: None
			}]
		);

		assert_eq!(
			lex_and_parse(
				"if false {
					create number = 1
				} else {
					create number = 2
				}"
			),
			vec![Statement::If {
				condition: ConditionBlock {
					expression: Expression::Bool(false),
					then: vec![Statement::CreateDeclaration { name: String::from("number"), initial: Some(Expression::Number(1.0)) },]
				},
				others_conditions: None,
				otherwise: Some(vec![Statement::CreateDeclaration { name: String::from("number"), initial: Some(Expression::Number(2.0)) },])
			}]
		);

		assert_eq!(
			lex_and_parse(
				"if false {
					create number = 3
				} elif true{
					create number = 6
				}else {
					create number = 9
				}"
			),
			vec![Statement::If {
				condition: ConditionBlock {
					expression: Expression::Bool(false),
					then: vec![Statement::CreateDeclaration { name: String::from("number"), initial: Some(Expression::Number(3.0)) },]
				},
				others_conditions: Some(vec![ConditionBlock {
					expression: Expression::Bool(true),
					then: vec![Statement::CreateDeclaration { name: String::from("number"), initial: Some(Expression::Number(6.0)) },]
				}]),
				otherwise: Some(vec![Statement::CreateDeclaration { name: String::from("number"), initial: Some(Expression::Number(9.0)) },])
			}]
		);
	}

	#[test]
	fn it_can_parse_while_statements() {
		assert_eq!(
			lex_and_parse("while true {}"),
			vec![Statement::While { condition: ConditionBlock { expression: Expression::Bool(true), then: vec![] } }]
		);

		assert_eq!(
			lex_and_parse(
				"while true {
					create number = 1
				}"
			),
			vec![Statement::While {
				condition: ConditionBlock {
					expression: Expression::Bool(true),
					then: vec![Statement::CreateDeclaration { name: String::from("number"), initial: Some(Expression::Number(1.0)) }]
				}
			}]
		);

		assert_eq!(
			lex_and_parse(
				"while true {
					break
					create number = 1
				}"
			),
			vec![Statement::While {
				condition: ConditionBlock {
					expression: Expression::Bool(true),
					then: vec![
						Statement::Break,
						Statement::CreateDeclaration { name: String::from("number"), initial: Some(Expression::Number(1.0)) },
					]
				},
			}]
		);

		assert_eq!(
			lex_and_parse(
				"while true {
					if true {
						continue
					}
					-- never touch this create number
					create number = 1
				}"
			),
			vec![Statement::While {
				condition: ConditionBlock {
					expression: Expression::Bool(true),
					then: vec![
						Statement::If {
							condition: ConditionBlock { expression: Expression::Bool(true), then: vec![Statement::Continue] },
							others_conditions: None,
							otherwise: None
						},
						Statement::CreateDeclaration { name: String::from("number"), initial: Some(Expression::Number(1.0)) },
					]
				},
			}]
		);
	}

	#[test]
	fn it_can_parse_loop_statements() {
		assert_eq!(lex_and_parse("loop {}"), vec![Statement::Loop { body: vec![] }]);

		assert_eq!(
			lex_and_parse(
				"loop {
					create number = 1
				}"
			),
			vec![Statement::Loop {
				body: vec![Statement::CreateDeclaration { name: String::from("number"), initial: Some(Expression::Number(1.0)) }]
			}]
		);

		assert_eq!(
			lex_and_parse(
				"loop {
					break
					create number = 1
				}"
			),
			vec![Statement::Loop {
				body: vec![Statement::Break, Statement::CreateDeclaration { name: String::from("number"), initial: Some(Expression::Number(1.0)) },]
			}]
		);

		assert_eq!(
			lex_and_parse(
				"loop {
					if true {
						continue
					}
					-- never touch this create number
					create number = 1
				}"
			),
			vec![Statement::Loop {
				body: vec![
					Statement::If {
						condition: ConditionBlock { expression: Expression::Bool(true), then: vec![Statement::Continue] },
						others_conditions: None,
						otherwise: None
					},
					Statement::CreateDeclaration { name: String::from("number"), initial: Some(Expression::Number(1.0)) },
				]
			}]
		);
	}

	#[test]
	fn it_can_parse_struct_declarations() {
		assert_eq!(
			lex_and_parse(
				"struct Point {
					x, y
				}"
			),
			vec![Statement::StructDeclaration {
				name: String::from("Point"),
				fields: vec![Parameter { name: String::from("x") }, Parameter { name: String::from("y") }]
			}]
		);

		let mut struct_fields: HashMap<Identifier, Expression> = HashMap::new();
		struct_fields.insert(Identifier::from("name"), Expression::Identifier("name".to_owned()));
		struct_fields.insert(Identifier::from("email"), Expression::Identifier("email".to_owned()));

		assert_eq!(
			lex_and_parse(
				"struct Person {
					name,
					email
				}

				Person.new = fn (name, email) {
					return Person { name, email }
				}"
			),
			vec![
				Statement::StructDeclaration {
					name: "Person".to_owned(),
					fields: vec![Parameter { name: "name".to_owned() }, Parameter { name: "email".to_owned() }]
				},
				Statement::Expression {
					expression: Expression::Assign(
						Box::new(Expression::Get(Box::new(Expression::Identifier("Person".to_owned())), "new".to_owned())),
						Box::new(Expression::Closure(
							vec![Parameter { name: "name".to_owned() }, Parameter { name: "email".to_owned() }],
							vec![Statement::Return {
								value: Expression::Struct(Box::new(Expression::Identifier("Person".to_owned())), struct_fields)
							}]
						))
					)
				}
			]
		);
	}
}
