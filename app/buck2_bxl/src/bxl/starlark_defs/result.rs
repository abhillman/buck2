/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under both the MIT license found in the
 * LICENSE-MIT file in the root directory of this source tree and the Apache
 * License, Version 2.0 found in the LICENSE-APACHE file in the root directory
 * of this source tree.
 */

use allocative::Allocative;
use derivative::Derivative;
use derive_more::Display;
use display_container::fmt_container;
use dupe::Dupe;
use starlark::any::ProvidesStaticType;
use starlark::environment::Methods;
use starlark::environment::MethodsBuilder;
use starlark::environment::MethodsStatic;
use starlark::starlark_complex_values;
use starlark::starlark_module;
use starlark::starlark_simple_value;
use starlark::values::starlark_value;
use starlark::values::string::StarlarkStr;
use starlark::values::Freeze;
use starlark::values::FrozenValue;
use starlark::values::NoSerialize;
use starlark::values::StarlarkValue;
use starlark::values::Trace;
use starlark::values::Value;
use starlark::values::ValueLike;
use starlark::values::ValueTypedComplex;
use starlark::StarlarkDocs;

#[derive(Debug, buck2_error::Error)]
enum BxlResultError {
    #[error("called `bxl.Result.unwrap()` on an `Err` value: {0}")]
    UnwrapOnError(buck2_error::Error),
    #[error("called `bxl.Result.unwrap_err()` on an `Ok` value: {0}")]
    UnwrapErrOnOk(String),
}

/// Error value object returned by fallible BXL operation.
#[derive(
    Debug,
    ProvidesStaticType,
    Derivative,
    Display,
    // TODO(nero): implement Serialize for StarlarkError
    NoSerialize,
    Allocative,
    StarlarkDocs,
    Trace
)]
#[starlark_docs(directory = "bxl")]
#[display("bx.Error({})", StarlarkStr::repr(&format!("{:?}", err)))]
pub(crate) struct StarlarkError {
    err: buck2_error::Error,
}

starlark_simple_value!(StarlarkError);

#[starlark_value(type = "bxl.Error")]
impl<'v> StarlarkValue<'v> for StarlarkError {
    fn get_methods() -> Option<&'static Methods> {
        static RES: MethodsStatic = MethodsStatic::new();
        RES.methods(error_methods)
    }
}

/// The error type for bxl
#[starlark_module]
fn error_methods(builder: &mut MethodsBuilder) {
    /// The error message
    #[starlark(attribute)]
    fn message<'v>(this: &'v StarlarkError) -> anyhow::Result<String> {
        Ok(format!("{:?}", this.err))
    }
}

#[derive(
    Debug,
    // TODO(nero): implement Serialize for StarlarkResult
    NoSerialize,
    Trace,
    Freeze,
    StarlarkDocs,
    ProvidesStaticType,
    Allocative
)]
#[starlark_docs(directory = "bxl")]
#[repr(C)]
pub(crate) enum StarlarkResultGen<T> {
    Ok(T),
    Err(#[freeze(identity)] buck2_error::Error),
}

pub(crate) type StarlarkResult<'v> = StarlarkResultGen<Value<'v>>;
pub(crate) type FrozenStarlarkResult = StarlarkResultGen<FrozenValue>;

starlark_complex_values!(StarlarkResult);

impl<T: Display> Display for StarlarkResultGen<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StarlarkResultGen::Ok(val) => fmt_container(f, "Result(Ok = ", ")", [val]),
            StarlarkResultGen::Err(err) => fmt_container(
                f,
                "Result(Err = ",
                ")",
                // TODO(nero): implement multiline when multiline is requested
                [StarlarkStr::repr(&format!("{:?}", err))],
            ),
        }
    }
}

#[starlark_value(type = "bxl.Result")]
impl<'v, V: ValueLike<'v>> StarlarkValue<'v> for StarlarkResultGen<V>
where
    Self: ProvidesStaticType<'v>,
{
    fn get_methods() -> Option<&'static Methods>
    where
        Self: Sized,
    {
        static RES: MethodsStatic = MethodsStatic::new();
        RES.methods(result_methods)
    }
}

#[starlark_module]
fn result_methods(builder: &mut MethodsBuilder) {
    /// Returns true if the result is an `Ok` value, false if it is an Error
    fn is_ok<'v>(this: ValueTypedComplex<'v, StarlarkResult<'v>>) -> anyhow::Result<bool> {
        Ok(match this.unpack() {
            either::Either::Left(x) => x.is_ok(),
            either::Either::Right(x) => x.is_ok(),
        })
    }

    /// Unwrap the result, returning the inner value if the result is `Ok`.
    /// If the result is an `Error`, it will fail
    fn unwrap<'v>(this: ValueTypedComplex<'v, StarlarkResult<'v>>) -> anyhow::Result<Value<'v>> {
        match this.unpack() {
            either::Either::Left(x) => x.unwrap(),
            either::Either::Right(x) => x.unwrap(),
        }
    }

    /// Unwrap the error, returning the inner error if the result is `Err`.
    /// If the result is an `Ok`, it will fail
    fn unwrap_err<'v>(
        this: ValueTypedComplex<'v, StarlarkResult<'v>>,
    ) -> anyhow::Result<StarlarkError> {
        match this.unpack() {
            either::Either::Left(x) => x.unwrap_err(),
            either::Either::Right(x) => x.unwrap_err(),
        }
    }
}

impl<T> StarlarkResultGen<T> {
    pub(crate) fn from_result(res: anyhow::Result<T>) -> Self {
        match res {
            Ok(val) => Self::Ok(val),
            Err(err) => Self::Err(buck2_error::Error::from(err)),
        }
    }

    fn is_ok(&self) -> bool {
        match self {
            StarlarkResultGen::Ok(_) => true,
            StarlarkResultGen::Err(_) => false,
        }
    }
}

impl<'v, V: ValueLike<'v>> StarlarkResultGen<V> {
    fn unwrap(&self) -> anyhow::Result<Value<'v>> {
        match self {
            StarlarkResultGen::Ok(val) => Ok(val.to_value()),
            StarlarkResultGen::Err(err) => Err(BxlResultError::UnwrapOnError(err.dupe()).into()),
        }
    }

    fn unwrap_err(&self) -> anyhow::Result<StarlarkError> {
        match self {
            StarlarkResultGen::Ok(val) => {
                let display_str = format!("{}", val);
                Err(BxlResultError::UnwrapErrOnOk(display_str).into())
            }
            StarlarkResultGen::Err(err) => Ok(StarlarkError { err: err.dupe() }),
        }
    }
}