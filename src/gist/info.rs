//! Gist info module.

#![allow(dead_code)]

use std::borrow::{Borrow, Cow};
use std::collections::BTreeMap;
use std::fmt;


custom_derive! {
    /// Enum listing all the recognized pieces of gist information.
    #[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd,
             IterVariants(Data))]
    pub enum Datum {
        /// Host-specific ID of the gist.
        Id,
        /// Name of the gist's owner.
        Owner,
        /// URL to the HTML page of the gist.
        Url,
        /// Description of the gist, typically provided by the owner upon creation.
        Description,
        /// Date/time the gist was created.
        CreatedAt,
        /// Date/time the gist was modified.
        UpdatedAt,
    }
}
impl Datum {
    pub fn default_value(&self) -> &'static str {
        match *self {
            Datum::Id |
            Datum::Owner |
            Datum::CreatedAt |
            Datum::UpdatedAt => "(unknown)",
            Datum::Url => "N/A",
            Datum::Description => "",
        }
    }
}
impl fmt::Display for Datum {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let msg = match *self {
            Datum::Id => "ID",
            Datum::Owner => "Owner",
            Datum::Url => "URL",
            Datum::Description => "Description",
            Datum::CreatedAt => "Created at",
            Datum::UpdatedAt => "Last update",
        };
        write!(fmt, "{}", msg)
    }
}

/// Type of gist info data values.
pub type Value = String;


/// Information about a particular gist.
#[derive(Clone, Debug)]
pub struct Info {
    data: BTreeMap<Datum, Value>,
}

impl Info {
    #[inline]
    pub fn has(&self, datum: Datum) -> bool {
        self.data.contains_key(&datum)
    }

    #[inline]
    pub fn get(&self, datum: Datum) -> Cow<Value> {
        match self.data.get(&datum) {
            Some(value) => Cow::Borrowed(value),
            None => Cow::Owned(datum.default_value().to_owned()),
        }
    }
}

impl fmt::Display for Info {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        // TODO: align all colons vertically
        for (datum, value) in &self.data {
            try!(writeln!(fmt, "{}: {}", datum, value));
        }
        Ok(())
    }
}


/// Builder for the gist Info struct.
#[derive(Clone, Debug)]
pub struct InfoBuilder {
    data: BTreeMap<Datum, Value>,
}

impl InfoBuilder {
    #[inline]
    pub fn new() -> Self {
        InfoBuilder{data: BTreeMap::new()}
    }

    #[inline]
    pub fn with<V: ?Sized>(mut self, datum: Datum, value: &V) -> Self
        where Value: Borrow<V>, V: ToOwned<Owned=Value>
    {
        self.set(datum, value); self
    }

    #[inline]
    pub fn without(mut self, datum: Datum) -> Self {
        self.unset(datum); self
    }

     #[inline]
    pub fn set<V: ?Sized>(&mut self, datum: Datum, value: &V) -> &mut Self
        where Value: Borrow<V>, V: ToOwned<Owned=Value>
    {
        self.data.insert(datum, value.to_owned());
        self
    }

    #[inline]
    pub fn unset(&mut self, datum: Datum) -> &mut Self {
        self.data.remove(&datum); self
    }

    #[inline]
    pub fn build(self) -> Info {
        Info{data: self.data}
    }
}


#[cfg(test)]
mod tests {
    use super::{Datum, InfoBuilder};

    #[test]
    fn datum_order_id_always_first() {
        let data: Vec<_> = Datum::iter_variants().collect();
        assert_eq!(Datum::Id, data[0]);
        for datum in data.into_iter().skip(1) {
            assert!(Datum::Id < datum);
        }
    }

    #[test]
    fn datum_order_dates_last() {
        const DATES_DATA: &'static [Datum] = &[Datum::CreatedAt, Datum::UpdatedAt];
        for datum in Datum::iter_variants() {
            if DATES_DATA.contains(&datum) {
                continue;
            }
            for &date_datum in DATES_DATA {
                assert!(datum < date_datum);
            }
        }
    }

    #[test]
    fn info_empty() {
        let info = InfoBuilder::new().build();
        for datum in Datum::iter_variants() {
            assert!(!info.has(datum));
            assert_eq!(datum.default_value(), *info.get(datum));
        }
    }

    #[test]
    fn info_regular() {
        let id = String::from("some_id");
        let info = InfoBuilder::new()
            .set(Datum::Id, &id)
            .set(Datum::Owner, "JohnDoe")
            .set(Datum::Description, "Amazing gist")
            .build();
        assert_eq!(id, *info.get(Datum::Id));
        assert_eq!("JohnDoe", *info.get(Datum::Owner));
        assert_eq!("Amazing gist", *info.get(Datum::Description));
    }
}
