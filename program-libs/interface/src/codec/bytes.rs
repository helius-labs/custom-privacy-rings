//! The byte cursor every frame parser reads through, a short read fails with the parser's own error.

use spr_interface::instruction::InterfaceTransfer;
use wincode::{config::DefaultConfig, containers, len::FixIntLen, SchemaRead};

use crate::error::CustomRingError;

/// A cursor whose short reads fail with `short`.
pub struct Bytes<'a, E = CustomRingError> {
    data: &'a [u8],
    short: E,
}

impl<'a, E: Copy> Bytes<'a, E> {
    pub const fn new(data: &'a [u8], short: E) -> Self {
        Self { data, short }
    }

    pub fn take(&mut self, len: usize) -> Result<&'a [u8], E> {
        let (head, rest) = self.data.split_at_checked(len).ok_or(self.short)?;
        self.data = rest;
        Ok(head)
    }

    pub fn array<const N: usize>(&mut self) -> Result<&'a [u8; N], E> {
        let (head, rest) = self.data.split_first_chunk::<N>().ok_or(self.short)?;
        self.data = rest;
        Ok(head)
    }

    pub fn byte(&mut self) -> Result<u8, E> {
        Ok(self.array::<1>()?[0])
    }

    /// `T` of alignment one, the cast then fails on a short read alone.
    pub fn pod<T: bytemuck::Pod>(&mut self) -> Result<&'a T, E> {
        bytemuck::try_from_bytes(self.take(core::mem::size_of::<T>())?).map_err(|_| self.short)
    }

    pub fn pod_slice<T: bytemuck::Pod>(&mut self, count: usize) -> Result<&'a [T], E> {
        const { assert!(core::mem::align_of::<T>() == 1) };
        let len = count
            .checked_mul(core::mem::size_of::<T>())
            .ok_or(self.short)?;
        bytemuck::try_cast_slice(self.take(len)?).map_err(|_| self.short)
    }

    /// `T`'s wincode image, a short or malformed read fails with `short`.
    pub fn schema<T: SchemaRead<'a, DefaultConfig, Dst = T>>(&mut self) -> Result<T, E> {
        T::get(&mut self.data).map_err(|_| self.short)
    }

    pub const fn rest(self) -> &'a [u8] {
        self.data
    }
}

impl<'a> Bytes<'a> {
    pub fn u16(&mut self) -> Result<u16, CustomRingError> {
        Ok(u16::from_le_bytes(*self.array()?))
    }

    pub fn u32(&mut self) -> Result<u32, CustomRingError> {
        Ok(u32::from_le_bytes(*self.array()?))
    }

    /// `ceil(len / 8)` bytes, bit `i` at byte `i / 8` bit `i % 8`, no bit set past `len`.
    pub fn bitmap(&mut self, len: usize) -> Result<u64, CustomRingError> {
        if len > u64::BITS as usize {
            return Err(self.short);
        }
        let bits = self
            .take(len.div_ceil(8))?
            .iter()
            .enumerate()
            .fold(0u64, |bits, (index, byte)| {
                bits | u64::from(*byte) << (8 * index)
            });
        if len < 64 && bits >> len != 0 {
            return Err(CustomRingError::InvalidInstructionData);
        }
        Ok(bits)
    }

    pub fn interface_transfers(&mut self) -> Result<Vec<InterfaceTransfer>, CustomRingError> {
        <containers::Vec<InterfaceTransfer, FixIntLen<u8>> as SchemaRead<'a, DefaultConfig>>::get(
            &mut self.data,
        )
        .map_err(|_| CustomRingError::InvalidInstructionData)
    }
}
