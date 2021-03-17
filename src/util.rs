use std::{
	fmt::{Debug, Display, Formatter},
	fmt,
	error::Error
};

#[derive(Debug, Clone)]
pub struct UnwrapError<T> {
	pub value: T,
	pub error_msg: String
}
impl<T> UnwrapError<T> {
	pub fn create(value: T, error_msg: String) -> Self {
		UnwrapError{value, error_msg}
	}
	
	pub fn strip_data(self) -> UnwrapError<()> {
		UnwrapError::create((), self.error_msg)
	}
}
impl<T> Display for UnwrapError<T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.error_msg)
	}
}
impl<T: Debug> Error for UnwrapError<T>{}

pub trait TryUnwrap<T> where Self: Sized {
	fn try_unwrap(self) -> Result<T, UnwrapError<Self>>;
}
impl<T> TryUnwrap<T> for Option<T> {
	fn try_unwrap(self) -> Result<T, UnwrapError<Self>> {
		if let Some(t) = self {
			Ok(t)
		} else {
			Err(UnwrapError::create(self, "called try_unwrap on none value".into()))
		}
	}
}