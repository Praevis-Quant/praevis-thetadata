//! Keyword arguments to typed request fields. Dates accept `datetime.date` or
//! `YYYY-MM-DD`; times accept `datetime.time` or vendor text; expirations a
//! date or text (such as `*`); symbol lists a string or a list of strings.
use chrono::{NaiveDate, NaiveTime};
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
    /// A required argument; `None` is not allowed.
    pub(crate) fn required<T: FromKw>(&mut self, name: &str) -> PyResult<T> {
        match self.get(name)? {
            Some(value) if !value.is_none() => T::from_kw(name, &value),
            _ => Err(PyTypeError::new_err(format!(
                "missing required argument: {name}"
            ))),
        }
    }
    /// An optional argument: absent keeps the default (`None`), an explicit
    /// `None` clears it (`Some(None)`).
    pub(crate) fn optional<T: FromKw>(&mut self, name: &str) -> PyResult<Option<Option<T>>> {
        match self.get(name)? {
            None => Ok(None),
            Some(value) if value.is_none() => Ok(Some(None)),
            Some(value) => T::from_kw(name, &value).map(|v| Some(Some(v))),
        }
    }
    /// A plain field: absent or `None` keeps the default.
    pub(crate) fn value<T: FromKw>(&mut self, name: &str) -> PyResult<Option<T>> {
        match self.get(name)? {
            Some(value) if !value.is_none() => T::from_kw(name, &value).map(Some),
            _ => Ok(None),
        }
    }
    /// Unknown keywords are errors, as in the vendor wrappers.
    pub(crate) fn finish(self) -> PyResult<()> {
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
