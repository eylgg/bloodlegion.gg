use std::convert::Infallible;

use crate::Error;

pub type Result<T, E = Infallible> = std::result::Result<T, Error<E>>;
