/*
Copied and modified from github.com/dnaq/sodiumoxide

Copyright (c) 2013 Daniel Ashhami

Permission is hereby granted, free of charge, to any person obtaining
a copy of this software and associated documentation files (the
"Software"), to deal in the Software without restriction, including
without limitation the rights to use, copy, modify, merge, publish,
distribute, sublicense, and/or sell copies of the Software, and to
permit persons to whom the Software is furnished to do so, subject to
the following conditions:

The above copyright notice and this permission notice shall be
included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE
LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION
WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

*/


macro_rules! newtype_clone (($newtype:ident) => (
        #[allow(clippy::non_canonical_clone_impl)]
        impl Clone for $newtype {
            fn clone(&self) -> $newtype {
                *self
            }
        }

        ));

macro_rules! newtype_from_slice (($newtype:ident, $len:expr) => (
    /// `from_slice()` creates an object from a byte slice
    ///
    /// This function will fail and return `None` if the length of
    /// the byte-s;ice isn't equal to the length of the object
    pub fn from_slice(bs: &[u8]) -> Option<$newtype> {
        if bs.len() != $len {
            return None;
        }
        let mut n = $newtype([0; $len]);
        {
            let $newtype(ref mut b) = n;
            for (bi, &bsi) in b.iter_mut().zip(bs.iter()) {
                *bi = bsi
            }
        }
        Some(n)
    }

	pub fn from_rng<R: Rng + ?Sized>(rng: &mut R) -> $newtype {
		let mut n = $newtype([0; $len]);
		{
			let $newtype(ref mut b) = n;
			rng.fill_bytes(b);
		}
		n
	}

    pub fn to_hex(&self) -> String {
        use ::data_encoding::HEXLOWER_PERMISSIVE;
        HEXLOWER_PERMISSIVE.encode(&self[..])
    }
));

macro_rules! newtype_traits (($newtype:ident, $len:expr) => (
	impl ::rand::distr::Distribution<$newtype> for ::rand::distr::StandardUniform {
		fn sample<R: ::rand::Rng + ?Sized>(&self, rng: &mut R) -> $newtype {
			$newtype::from_rng(rng)
		}
	}

    impl ::subtle::ConstantTimeEq for $newtype {
        fn ct_eq(&self, other: &Self) -> ::subtle::Choice {
            let &$newtype(ref v) = self;
            let &$newtype(ref o) = other;
            v.ct_eq(o)
        }
    }

    impl ::std::cmp::PartialEq for $newtype {
        fn eq(&self, other: &Self) -> bool {
            use ::subtle::ConstantTimeEq;

            self.ct_eq(other).into()
        }
    }

    impl<'a, 'b> ::std::cmp::PartialEq<$newtype> for &'b $newtype {
        fn eq(&self, other: &$newtype) -> bool {
            use ::subtle::ConstantTimeEq;

            self.ct_eq(other).into()
        }
    }

    impl ::std::cmp::Eq for $newtype {}

	impl ::serde::Serialize for $newtype {
		fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
			where S: ::serde::Serializer
		{
            use ::data_encoding::HEXLOWER_PERMISSIVE;
            serializer.serialize_str(&HEXLOWER_PERMISSIVE.encode(&self[..]))
		}
	}

	impl<'de> ::serde::Deserialize<'de> for $newtype {
		fn deserialize<D>(deserializer: D) -> Result<$newtype, D::Error>
			where D: ::serde::Deserializer<'de>
		{
			struct NewtypeVisitor;
			impl<'de> ::serde::de::Visitor<'de> for NewtypeVisitor {
				type Value = $newtype;
				fn expecting(&self, formatter: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
					write!(formatter, stringify!($newtype))
				}

                fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
                    where E: ::serde::de::Error
                {
                    use ::data_encoding::HEXLOWER_PERMISSIVE;
                    let slice = HEXLOWER_PERMISSIVE.decode(v.as_bytes()).map_err(::serde::de::Error::custom)?;
                    $newtype::from_slice(&slice).ok_or(::serde::de::Error::invalid_length(slice.len(), &self))
                }
			}
			deserializer.deserialize_str(NewtypeVisitor)
		}
	}

    /// Allows a user to access the byte contents of an object as a slice.
    ///
    /// WARNING: it might be tempting to do comparisons on objects
    /// by using `x[a..b] == y[a..b]`. This will open up for timing attacks
    /// when comparing for example authenticator tags. Because of this only
    /// use the comparison functions exposed by the sodiumoxide API.
    impl ::std::ops::Index<::std::ops::Range<usize>> for $newtype {
        type Output = [u8];
        fn index(&self, _index: ::std::ops::Range<usize>) -> &[u8] {
            let &$newtype(ref b) = self;
            b.index(_index)
        }
    }
    /// Allows a user to access the byte contents of an object as a slice.
    ///
    /// WARNING: it might be tempting to do comparisons on objects
    /// by using `x[..b] == y[..b]`. This will open up for timing attacks
    /// when comparing for example authenticator tags. Because of this only
    /// use the comparison functions exposed by the sodiumoxide API.
    impl ::std::ops::Index<::std::ops::RangeTo<usize>> for $newtype {
        type Output = [u8];
        fn index(&self, _index: ::std::ops::RangeTo<usize>) -> &[u8] {
            let &$newtype(ref b) = self;
            b.index(_index)
        }
    }
    /// Allows a user to access the byte contents of an object as a slice.
    ///
    /// WARNING: it might be tempting to do comparisons on objects
    /// by using `x[a..] == y[a..]`. This will open up for timing attacks
    /// when comparing for example authenticator tags. Because of this only
    /// use the comparison functions exposed by the sodiumoxide API.
    impl ::std::ops::Index<::std::ops::RangeFrom<usize>> for $newtype {
        type Output = [u8];
        fn index(&self, _index: ::std::ops::RangeFrom<usize>) -> &[u8] {
            let &$newtype(ref b) = self;
            b.index(_index)
        }
    }
    /// Allows a user to access the byte contents of an object as a slice.
    ///
    /// WARNING: it might be tempting to do comparisons on objects
    /// by using `x[] == y[]`. This will open up for timing attacks
    /// when comparing for example authenticator tags. Because of this only
    /// use the comparison functions exposed by the sodiumoxide API.
    impl ::std::ops::Index<::std::ops::RangeFull> for $newtype {
        type Output = [u8];
        fn index(&self, _index: ::std::ops::RangeFull) -> &[u8] {
            let &$newtype(ref b) = self;
            b.index(_index)
        }
    }
    impl ::std::fmt::Debug for $newtype  {
        fn fmt(&self,
               formatter: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
            write!(formatter, "{}({:?})", stringify!($newtype), &self[..])
        }
    }
    ));

macro_rules! public_newtype_traits (($newtype:ident) => (
    impl AsRef<[u8]> for $newtype {
        #[inline]
        fn as_ref(&self) -> &[u8] {
            &self[..]
        }
    }
    impl ::std::cmp::PartialOrd for $newtype {
        #[inline]
        fn partial_cmp(&self,
                       other: &$newtype) -> Option<::std::cmp::Ordering> {
            Some(self.cmp(other))
        }
        #[inline]
        fn lt(&self, other: &$newtype) -> bool {
            ::std::cmp::PartialOrd::lt(self.as_ref(), other.as_ref())
        }
        #[inline]
        fn le(&self, other: &$newtype) -> bool {
            ::std::cmp::PartialOrd::le(self.as_ref(), other.as_ref())
        }
        #[inline]
        fn ge(&self, other: &$newtype) -> bool {
            ::std::cmp::PartialOrd::ge(self.as_ref(), other.as_ref())
        }
        #[inline]
        fn gt(&self, other: &$newtype) -> bool {
            ::std::cmp::PartialOrd::gt(self.as_ref(), other.as_ref())
        }
    }
    impl ::std::cmp::Ord for $newtype {
        #[inline]
        fn cmp(&self, other: &$newtype) -> ::std::cmp::Ordering {
            ::std::cmp::Ord::cmp(self.as_ref(), other.as_ref())
        }
    }
    impl ::std::hash::Hash for $newtype {
        fn hash<H: ::std::hash::Hasher>(&self, state: &mut H) {
            ::std::hash::Hash::hash(self.as_ref(), state)
        }
    }
    ));

/// Macro used for generating newtypes of byte-arrays
macro_rules! new_type {
    ( $(#[$meta:meta])*
      secret $name:ident($bytes:expr);
      ) => (
        $(#[$meta])*
        #[must_use]
        pub struct $name(pub [u8; $bytes]);
        impl Clone for $name {
            fn clone(&self) -> $name {
                let &$name(v) = self;
                $name(v)
            }
        }
        newtype_traits!($name, $bytes);
        impl $name {
            newtype_from_slice!($name, $bytes);
        }
        impl Drop for $name {
            fn drop(&mut self) {
				use crypto::util::secure_memset;
                let &mut $name(ref mut v) = self;
				secure_memset(v, 0);
            }
        }
        );
    ( $(#[$meta:meta])*
      public $name:ident($bytes:expr);
      ) => (
        $(#[$meta])*
        #[derive(Copy)]
        #[must_use]
        pub struct $name(pub [u8; $bytes]);
        newtype_clone!($name);
        newtype_traits!($name, $bytes);
        public_newtype_traits!($name);
        impl $name {
            newtype_from_slice!($name, $bytes);
        }
        );
    ( $(#[$meta:meta])*
      nonce $name:ident($bytes:expr);
      ) => (
        $(#[$meta])*
        #[derive(Copy)]
        #[must_use]
        pub struct $name(pub [u8; $bytes]);
        newtype_clone!($name);
        newtype_traits!($name, $bytes);
        public_newtype_traits!($name);
        impl $name {
            newtype_from_slice!($name, $bytes);

            /// `increment_le()` treats the nonce as an unsigned little-endian number and
            /// returns an incremented version of it.
            ///
            /// WARNING: this method does not check for arithmetic overflow. It is the callers
            /// responsibility to ensure that any given nonce value is only used once.
            /// If the caller does not do that the cryptographic primitives in sodiumoxide
            /// will not uphold any security guarantees (i.e. they will break)
            pub fn increment_le(&self) -> $name {
                let mut res = *self;
                res.increment_le_inplace();
                res
            }

            /// `increment_le_inplace()` treats the nonce as an unsigned little-endian number
            /// and increments it.
            ///
            /// WARNING: this method does not check for arithmetic overflow. It is the callers
            /// responsibility to ensure that any given nonce value is only used once.
            /// If the caller does not do that the cryptographic primitives in sodiumoxide
            /// will not uphold any security guarantees.
            pub fn increment_le_inplace(&mut self) {
                use utils::increment_le;
                let &mut $name(ref mut r) = self;
                increment_le(r);
            }

        }
        );
}
