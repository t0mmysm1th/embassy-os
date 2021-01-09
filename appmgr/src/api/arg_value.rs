use std::borrow::Cow;
use std::num::{ParseFloatError, ParseIntError};
use std::str::{FromStr, ParseBoolError};

use hashlink::LinkedHashMap as Map;
use serde::{
    de::{
        DeserializeSeed, EnumAccess, Error, IntoDeserializer, MapAccess, SeqAccess, Unexpected,
        VariantAccess, Visitor,
    },
    Deserialize, Deserializer, Serialize,
};

pub type QueryMap<'a> = Map<Cow<'a, str>, ArgValue<'a>>;

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ArgValue<'a> {
    Str(Cow<'a, str>),
    Arr(Vec<ArgValue<'a>>),
    Map(QueryMap<'a>),
}
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Parse Error: {0}")]
    Bool(#[from] ParseBoolError),
    #[error("Parse Error: {0}")]
    Int(#[from] ParseIntError),
    #[error("Parse Error: {0}")]
    Float(#[from] ParseFloatError),
    #[error("Parse Error: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("Parse Error: {0}")]
    Custom(String),
    #[error("Parse Error: Can't Parse Array")]
    Array,
    #[error("Parse Error: Can't Parse Map")]
    Map,
    #[error("Parse Error: Not An Array")]
    NotArray,
    #[error("Parse Error: Not A Map")]
    NotMap,
}
impl Error for ParseError {
    fn custom<T>(msg: T) -> Self
    where
        T: std::fmt::Display,
    {
        ParseError::Custom(format!("{}", msg))
    }
}
impl<'a> ArgValue<'a> {
    pub fn parse<T: FromStr>(&self) -> Result<T, ParseError>
    where
        ParseError: From<T::Err>,
    {
        match self {
            ArgValue::Str(s) => s.parse().map_err(ParseError::from),
            ArgValue::Arr(_) => Err(ParseError::Array),
            ArgValue::Map(_) => Err(ParseError::Map),
        }
    }
}
impl<'de> Deserialize<'de> for ArgValue<'de> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ArgValueVisitor;

        impl<'de> Visitor<'de> for ArgValueVisitor {
            type Value = ArgValue<'de>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("any valid query string value")
            }

            #[inline]
            fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(ArgValue::Str(Cow::Borrowed(value)))
            }

            #[inline]
            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(ArgValue::Str(Cow::Owned(value.to_owned())))
            }

            #[inline]
            fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
                Ok(ArgValue::Str(Cow::Owned(value)))
            }

            #[inline]
            fn visit_seq<V>(self, mut visitor: V) -> Result<Self::Value, V::Error>
            where
                V: SeqAccess<'de>,
            {
                let mut vec = Vec::new();

                while let Some(elem) = visitor.next_element()? {
                    vec.push(elem);
                }

                Ok(ArgValue::Arr(vec))
            }

            fn visit_map<V>(self, mut visitor: V) -> Result<Self::Value, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut map = Map::new();
                while let Some((key, val)) = visitor.next_entry()? {
                    map.insert(key, val);
                }
                Ok(ArgValue::Map(map))
            }
        }

        deserializer.deserialize_any(ArgValueVisitor)
    }
}
macro_rules! forward_parsable_to_deserialize_any {
    ($($ty:ident => $meth:ident,)*) => {
        $(
            fn $meth<V>(self, visitor: V) -> Result<V::Value, Self::Error> where V: Visitor<'de> {
                self.parse::<$ty>()?.into_deserializer().$meth(visitor)
            }
        )*
    }
}
struct MapDeserializer<'a>(
    hashlink::linked_hash_map::Iter<'a, Cow<'a, str>, ArgValue<'a>>,
    Option<&'a ArgValue<'a>>,
);
impl<'de> MapAccess<'de> for MapDeserializer<'de> {
    type Error = ParseError;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: DeserializeSeed<'de>,
    {
        if let Some((k, v)) = self.0.next() {
            self.1 = Some(v);
            Ok(Some(seed.deserialize(StrDeserializer(k.as_ref()))?))
        } else {
            Ok(None)
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        Ok(seed.deserialize(self.1.take().unwrap())?)
    }
}

struct SeqDeserializer<'a>(std::slice::Iter<'a, ArgValue<'a>>);
impl<'de> SeqAccess<'de> for SeqDeserializer<'de> {
    type Error = ParseError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        if let Some(elem) = self.0.next() {
            Ok(Some(seed.deserialize(elem)?))
        } else {
            Ok(None)
        }
    }

    fn size_hint(&self) -> Option<usize> {
        match self.0.size_hint() {
            (lower, Some(upper)) if lower == upper => Some(upper),
            _ => None,
        }
    }
}
impl<'de> serde::Deserializer<'de> for SeqDeserializer<'de> {
    type Error = ParseError;

    #[inline]
    fn deserialize_any<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let len = self.0.len();
        if len == 0 {
            visitor.visit_unit()
        } else {
            let ret = visitor.visit_seq(&mut self)?;
            let remaining = self.0.len();
            if remaining == 0 {
                Ok(ret)
            } else {
                Err(serde::de::Error::invalid_length(
                    len,
                    &"fewer elements in array",
                ))
            }
        }
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

struct VariantDeserializer<'a> {
    value: Option<&'a ArgValue<'a>>,
}

impl<'de> VariantAccess<'de> for VariantDeserializer<'de> {
    type Error = ParseError;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.value {
            Some(value) => Deserialize::deserialize(value),
            None => Ok(()),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.value {
            Some(value) => seed.deserialize(value),
            None => Err(Error::invalid_type(
                Unexpected::UnitVariant,
                &"newtype variant",
            )),
        }
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            Some(ArgValue::Arr(a)) => {
                serde::Deserializer::deserialize_any(SeqDeserializer(a.iter()), visitor)
            }
            Some(ArgValue::Map(_)) => Err(Error::invalid_type(Unexpected::Map, &"tuple variant")),
            Some(ArgValue::Str(s)) => {
                Err(Error::invalid_type(Unexpected::Str(&s), &"tuple variant"))
            }
            None => Err(Error::invalid_type(
                Unexpected::UnitVariant,
                &"tuple variant",
            )),
        }
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.value {
            Some(ArgValue::Map(m)) => visitor.visit_map(MapDeserializer(m.iter(), None)),
            Some(ArgValue::Arr(_)) => Err(Error::invalid_type(Unexpected::Seq, &"tuple variant")),
            Some(ArgValue::Str(s)) => {
                Err(Error::invalid_type(Unexpected::Str(&s), &"tuple variant"))
            }
            None => Err(Error::invalid_type(
                Unexpected::UnitVariant,
                &"struct variant",
            )),
        }
    }
}

struct EnumDeserializer<'a> {
    variant: &'a str,
    value: Option<&'a ArgValue<'a>>,
}

impl<'de> EnumAccess<'de> for EnumDeserializer<'de> {
    type Error = ParseError;
    type Variant = VariantDeserializer<'de>;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, VariantDeserializer<'de>), Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        let variant = self.variant.into_deserializer();
        let visitor = VariantDeserializer { value: self.value };
        seed.deserialize(variant).map(|v| (v, visitor))
    }
}
impl<'de> Deserializer<'de> for &'de ArgValue<'de> {
    type Error = ParseError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            ArgValue::Str(_) => self.deserialize_str(visitor),
            ArgValue::Arr(_) => self.deserialize_seq(visitor),
            ArgValue::Map(_) => self.deserialize_map(visitor),
        }
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            ArgValue::Map(m) => visitor.visit_map(MapDeserializer(m.iter(), None)),
            _ => Err(ParseError::NotMap),
        }
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            ArgValue::Arr(a) => visitor.visit_seq(SeqDeserializer(a.iter())),
            _ => Err(ParseError::NotArray),
        }
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            ArgValue::Str(s) if s.len() == 1 => visitor.visit_char(s.chars().next().unwrap()),
            ArgValue::Str(s) => Err(ParseError::invalid_length(s.len(), &"a single character")),
            ArgValue::Arr(_) => Err(ParseError::Array),
            ArgValue::Map(_) => Err(ParseError::Map),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            ArgValue::Str(s) => visitor.visit_borrowed_str(s.as_ref()),
            ArgValue::Arr(_) => Err(ParseError::Array),
            ArgValue::Map(_) => Err(ParseError::Map),
        }
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            ArgValue::Str(s) => visitor.visit_string(s.to_string()),
            ArgValue::Arr(_) => Err(ParseError::Array),
            ArgValue::Map(_) => Err(ParseError::Map),
        }
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            ArgValue::Str(Cow::Borrowed(s)) => visitor.visit_bytes(&base64::decode_config(
                s,
                base64::Config::new(base64::CharacterSet::UrlSafe, true),
            )?),
            ArgValue::Str(Cow::Owned(s)) => visitor.visit_bytes(&base64::decode(s)?),
            ArgValue::Arr(_) => Err(ParseError::Array),
            ArgValue::Map(_) => Err(ParseError::Map),
        }
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            ArgValue::Str(Cow::Borrowed(s)) => visitor.visit_byte_buf(base64::decode_config(
                s,
                base64::Config::new(base64::CharacterSet::UrlSafe, true),
            )?),
            ArgValue::Str(Cow::Owned(s)) => visitor.visit_byte_buf(base64::decode(s)?),
            ArgValue::Arr(_) => Err(ParseError::Array),
            ArgValue::Map(_) => Err(ParseError::Map),
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit_struct("", visitor)
    }

    fn deserialize_unit_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            ArgValue::Str(s) => {
                if s == name {
                    visitor.visit_unit()
                } else {
                    Err(Error::invalid_value(Unexpected::Str(s.as_ref()), &name))
                }
            }
            ArgValue::Arr(_) => Err(ParseError::Array),
            ArgValue::Map(_) => Err(ParseError::Map),
        }
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_tuple(len, visitor)
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        use std::slice::Iter;
        struct MapDeserializer<'a>(
            Iter<'a, &'static str>,
            &'a QueryMap<'a>,
            Option<&'a ArgValue<'a>>,
        );
        impl<'de> MapAccess<'de> for MapDeserializer<'de> {
            type Error = ParseError;

            fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
            where
                K: DeserializeSeed<'de>,
            {
                if let Some(k) = self.0.next() {
                    if let Some(v) = self.1.get(*k) {
                        self.2 = Some(v);
                        Ok(Some(seed.deserialize(StrDeserializer(*k))?))
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }

            fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
            where
                V: DeserializeSeed<'de>,
            {
                Ok(seed.deserialize(self.2.take().unwrap())?)
            }
        }

        match self {
            ArgValue::Map(m) => visitor.visit_map(MapDeserializer(fields.iter(), m, None)),
            _ => Err(ParseError::NotMap),
        }
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let (variant, value) = match self {
            ArgValue::Map(value) => {
                let mut iter = value.into_iter();
                let (variant, value) = match iter.next() {
                    Some(v) => v,
                    None => {
                        return Err(Error::invalid_value(
                            Unexpected::Map,
                            &"map with a single key",
                        ));
                    }
                };
                // enums are encoded in json as maps with a single key:value pair
                if iter.next().is_some() {
                    return Err(Error::invalid_value(
                        Unexpected::Map,
                        &"map with a single key",
                    ));
                }
                (variant, Some(value))
            }
            ArgValue::Str(variant) => (variant, None),
            ArgValue::Arr(_) => {
                return Err(Error::invalid_type(Unexpected::Seq, &"string or map"));
            }
        };

        visitor.visit_enum(EnumDeserializer { variant, value })
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }

    serde::forward_to_deserialize_any! {
        option
    }

    forward_parsable_to_deserialize_any! {
        bool => deserialize_bool,
        u8 => deserialize_u8,
        u16 => deserialize_u16,
        u32 => deserialize_u32,
        u64 => deserialize_u64,
        i8 => deserialize_i8,
        i16 => deserialize_i16,
        i32 => deserialize_i32,
        i64 => deserialize_i64,
        f32 => deserialize_f32,
        f64 => deserialize_f64,
    }
}
struct StrDeserializer<'a>(&'a str);
impl<'a> StrDeserializer<'a> {
    pub fn parse<T: FromStr>(&self) -> Result<T, ParseError>
    where
        ParseError: From<T::Err>,
    {
        self.0.parse().map_err(ParseError::from)
    }
}
impl<'de> serde::Deserializer<'de> for StrDeserializer<'de> {
    type Error = ParseError;

    #[inline]
    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_borrowed_str(self.0.as_ref())
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if self.0.len() == 1 {
            visitor.visit_char(self.0.chars().next().unwrap())
        } else {
            Err(ParseError::invalid_length(
                self.0.len(),
                &"a single character",
            ))
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_borrowed_str(self.0.as_ref())
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_string(self.0.to_string())
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_bytes(&base64::decode_config(
            self.0,
            base64::Config::new(base64::CharacterSet::UrlSafe, true),
        )?)
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_byte_buf(base64::decode_config(
            self.0,
            base64::Config::new(base64::CharacterSet::UrlSafe, true),
        )?)
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit_struct("", visitor)
    }

    fn deserialize_unit_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if self.0 == name {
            visitor.visit_unit()
        } else {
            Err(Error::invalid_value(
                Unexpected::Str(self.0.as_ref()),
                &name,
            ))
        }
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_enum(EnumDeserializer {
            variant: self.0,
            value: None,
        })
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }

    serde::forward_to_deserialize_any! {
        option
        seq
        tuple
        tuple_struct
        map
        struct
    }

    forward_parsable_to_deserialize_any! {
        bool => deserialize_bool,
        u8 => deserialize_u8,
        u16 => deserialize_u16,
        u32 => deserialize_u32,
        u64 => deserialize_u64,
        i8 => deserialize_i8,
        i16 => deserialize_i16,
        i32 => deserialize_i32,
        i64 => deserialize_i64,
        f32 => deserialize_f32,
        f64 => deserialize_f64,
    }
}

impl<'de> IntoDeserializer<'de, ParseError> for &'de ArgValue<'de> {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self::Deserializer {
        self
    }
}
