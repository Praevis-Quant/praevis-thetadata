//! Keyword arguments to typed request fields. Dates accept `datetime.date` or
//! `YYYY-MM-DD`; times accept `datetime.time` or vendor text; expirations a
//! date or text (such as `*`); symbol lists a string or a list of strings.
use chrono::{NaiveDate, NaiveTime};
use praevis_thetadata_client::dispatch::{ParamKind, ParamValue, Params};
use praevis_thetadata_client::endpoints::{Expiration, Symbols, TimeOfDay};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString};
use std::collections::BTreeSet;

pub(crate) trait FromKw: Sized {
    fn from_kw(name: &str, value: &Bound<'_, PyAny>) -> PyResult<Self>;
}

fn wrong(name: &str, expected: &str) -> PyErr {
    PyTypeError::new_err(format!("{name}: expected {expected}"))
}

fn date(name: &str, value: &Bound<'_, PyAny>) -> PyResult<NaiveDate> {
    if let Ok(text) = value.cast::<PyString>() {
        return NaiveDate::parse_from_str(text.to_str()?, "%Y-%m-%d")
            .map_err(|_| PyValueError::new_err(format!("{name}: expected YYYY-MM-DD")));
    }
    // A datetime is a date subclass in Python; refuse it rather than drop the time.
    if value.hasattr("hour")? {
        return Err(wrong(name, "datetime.date, not datetime.datetime"));
    }
    value
        .extract::<NaiveDate>()
        .map_err(|_| wrong(name, "datetime.date or YYYY-MM-DD"))
}

impl FromKw for String {
    fn from_kw(name: &str, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        value.extract().map_err(|_| wrong(name, "str"))
    }
}
impl FromKw for f64 {
    fn from_kw(name: &str, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        value.extract().map_err(|_| wrong(name, "float"))
    }
}
impl FromKw for i32 {
    fn from_kw(name: &str, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        if value.is_instance_of::<pyo3::types::PyBool>() {
            return Err(wrong(name, "int"));
        }
        value.extract().map_err(|_| wrong(name, "int"))
    }
}
impl FromKw for bool {
    fn from_kw(name: &str, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        value.extract().map_err(|_| wrong(name, "bool"))
    }
}
impl FromKw for NaiveDate {
    fn from_kw(name: &str, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        date(name, value)
    }
}
impl FromKw for Expiration {
    fn from_kw(name: &str, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(text) = value.cast::<PyString>() {
            return Ok(Expiration::Text(text.to_str()?.to_owned()));
        }
        date(name, value).map(Expiration::Date)
    }
}
impl FromKw for TimeOfDay {
    fn from_kw(name: &str, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(text) = value.cast::<PyString>() {
            return Ok(TimeOfDay::Text(text.to_str()?.to_owned()));
        }
        value
            .extract::<NaiveTime>()
            .map(TimeOfDay::Time)
            .map_err(|_| wrong(name, "datetime.time or str"))
    }
}
impl FromKw for Vec<String> {
    fn from_kw(name: &str, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(text) = value.cast::<PyString>() {
            return Ok(vec![text.to_str()?.to_owned()]);
        }
        value
            .extract()
            .map_err(|_| wrong(name, "str or list of str"))
    }
}
impl FromKw for Symbols {
    fn from_kw(name: &str, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Vec::<String>::from_kw(name, value).map(Symbols)
    }
}

/// The caller's keyword arguments, consumed by name; leftovers are errors.
pub(crate) struct Kwargs<'a, 'py> {
    dict: Option<&'a Bound<'py, PyDict>>,
    used: BTreeSet<String>,
}
impl<'a, 'py> Kwargs<'a, 'py> {
    pub(crate) fn new(dict: Option<&'a Bound<'py, PyDict>>) -> Self {
        Self {
            dict,
            used: BTreeSet::new(),
        }
    }
    fn get(&mut self, name: &str) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.used.insert(name.to_owned());
        match self.dict {
            Some(dict) => dict.get_item(name),
            None => Ok(None),
        }
    }
    /// Unknown keywords are errors, as in the vendor wrappers.
    fn unknown(self) -> PyResult<()> {
        let Some(dict) = self.dict else {
            return Ok(());
        };
        let unknown: Vec<String> = dict
            .keys()
            .iter()
            .filter_map(|k| k.extract::<String>().ok())
            .filter(|k| !self.used.contains(k))
            .collect();
        if unknown.is_empty() {
            Ok(())
        } else {
            Err(PyTypeError::new_err(format!(
                "unexpected keyword arguments: {}",
                unknown.join(", ")
            )))
        }
    }
}

/// Keyword arguments as the client's generated dispatch reads them: the same
/// Python conversions and error messages as the bindings always had.
impl Params for Kwargs<'_, '_> {
    type Error = PyErr;
    fn take(&mut self, name: &str, kind: ParamKind) -> PyResult<Option<ParamValue>> {
        let Some(value) = self.get(name)? else {
            return Ok(None);
        };
        if value.is_none() {
            return Ok(Some(ParamValue::Null));
        }
        let value = &value;
        Ok(Some(match kind {
            ParamKind::Str => ParamValue::Str(String::from_kw(name, value)?),
            // Extracted as i32, as before: an out-of-range int is "expected int".
            ParamKind::Int => ParamValue::Int(i64::from(i32::from_kw(name, value)?)),
            ParamKind::Float => ParamValue::Float(f64::from_kw(name, value)?),
            ParamKind::Bool => ParamValue::Bool(bool::from_kw(name, value)?),
            ParamKind::Date => ParamValue::Date(NaiveDate::from_kw(name, value)?),
            ParamKind::Time => ParamValue::Time(TimeOfDay::from_kw(name, value)?),
            ParamKind::Expiration => ParamValue::Expiration(Expiration::from_kw(name, value)?),
            ParamKind::Symbols => ParamValue::Strings(Symbols::from_kw(name, value)?.0),
            ParamKind::StrList => ParamValue::Strings(Vec::<String>::from_kw(name, value)?),
            other => {
                return Err(PyTypeError::new_err(format!(
                    "{name}: unsupported parameter kind {other:?}"
                )));
            }
        }))
    }
    fn finish(self) -> PyResult<()> {
        self.unknown()
    }
}
