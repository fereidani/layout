//! Pointer cursors for the generated single-counter iterators.
//!
//! A generated `FooIter` keeps one remaining-length counter and hands every
//! column a cursor; `next` checks the counter once and then advances each
//! cursor unchecked. The obvious column cursor is `slice::Iter`, but its
//! `next` carries its own `ptr == end` test, and
//! `next().unwrap_unchecked()` does not reliably fold that test away: for a
//! wide struct LLVM speculates both arms instead, emitting a compare, a
//! conditional pointer update and a spilled flag *per column, per element*.
//!
//! [`ColumnCursor`] carries no end state, so there is no test to fold: the
//! cursor is a bare pointer bump, and columns the loop body never reads
//! become dead induction variables that LLVM deletes outright.

use core::{marker::PhantomData, ptr::NonNull};

use crate::SoACursor;

/// Shared-reference cursor over one plain SOA column.
///
/// Yields `&'a T` in front-to-back or back-to-front order without any
/// exhaustion check; the enclosing iterator's counter is the only bound.
pub struct ColumnCursor<'a, T> {
    front: NonNull<T>,
    back: NonNull<T>,
    _marker: PhantomData<&'a [T]>,
}

impl<'a, T> ColumnCursor<'a, T> {
    /// Build a cursor over `slice`, positioned at both of its ends.
    #[inline]
    pub fn new(slice: &'a [T]) -> Self {
        let front = NonNull::from(slice).cast::<T>();
        // SAFETY: `front` is the base of a `len`-element slice, so the
        // one-past-the-end pointer is in bounds of the same allocation and
        // non-null. For a ZST the offset is zero bytes and `back == front`.
        let back =
            unsafe { NonNull::new_unchecked(front.as_ptr().add(slice.len())) };
        ColumnCursor {
            front,
            back,
            _marker: PhantomData,
        }
    }
}

impl<'a, T> SoACursor for ColumnCursor<'a, T> {
    type Item = &'a T;

    #[inline(always)]
    unsafe fn cursor_next(&mut self) -> &'a T {
        let current = self.front.as_ptr();
        // SAFETY: the caller guarantees an unyielded element remains, so
        // `front` points at a live element and stepping past it stays within
        // the column, keeping the pointer non-null.
        self.front = unsafe { NonNull::new_unchecked(current.add(1)) };
        // SAFETY: as above; the element is live for `'a` and only shared
        // references to it are handed out.
        unsafe { &*current }
    }

    #[inline(always)]
    unsafe fn cursor_next_back(&mut self) -> &'a T {
        // SAFETY: the caller guarantees an unyielded element remains, so
        // `back` is one past a live element; stepping back to it keeps the
        // pointer non-null.
        let current = unsafe { self.back.as_ptr().sub(1) };
        self.back = unsafe { NonNull::new_unchecked(current) };
        // SAFETY: as in `cursor_next`.
        unsafe { &*current }
    }
}

impl<T> Clone for ColumnCursor<'_, T> {
    #[inline]
    fn clone(&self) -> Self {
        ColumnCursor {
            front: self.front,
            back: self.back,
            _marker: PhantomData,
        }
    }
}

// SAFETY: the cursor hands out `&T`, exactly like the `&'a [T]` it borrows.
unsafe impl<T: Sync> Send for ColumnCursor<'_, T> {}
// SAFETY: as above.
unsafe impl<T: Sync> Sync for ColumnCursor<'_, T> {}

/// Unique-reference cursor over one plain SOA column.
///
/// The mutable analog of [`ColumnCursor`], yielding `&'a mut T`.
pub struct ColumnCursorMut<'a, T> {
    front: NonNull<T>,
    back: NonNull<T>,
    _marker: PhantomData<&'a mut [T]>,
}

impl<'a, T> ColumnCursorMut<'a, T> {
    /// Build a cursor over `slice`, positioned at both of its ends.
    #[inline]
    pub fn new(slice: &'a mut [T]) -> Self {
        let len = slice.len();
        let front = NonNull::from(slice).cast::<T>();
        // SAFETY: as in `ColumnCursor::new`.
        let back = unsafe { NonNull::new_unchecked(front.as_ptr().add(len)) };
        ColumnCursorMut {
            front,
            back,
            _marker: PhantomData,
        }
    }
}

impl<'a, T> SoACursor for ColumnCursorMut<'a, T> {
    type Item = &'a mut T;

    #[inline(always)]
    unsafe fn cursor_next(&mut self) -> &'a mut T {
        let current = self.front.as_ptr();
        // SAFETY: as in `ColumnCursor::cursor_next`.
        self.front = unsafe { NonNull::new_unchecked(current.add(1)) };
        // SAFETY: as above. The two ends never cross (the caller yields each
        // element at most once), so the returned reference is unique.
        unsafe { &mut *current }
    }

    #[inline(always)]
    unsafe fn cursor_next_back(&mut self) -> &'a mut T {
        // SAFETY: as in `ColumnCursor::cursor_next_back`.
        let current = unsafe { self.back.as_ptr().sub(1) };
        self.back = unsafe { NonNull::new_unchecked(current) };
        // SAFETY: as in `cursor_next`.
        unsafe { &mut *current }
    }
}

// SAFETY: the cursor hands out `&mut T`, exactly like the `&'a mut [T]` it
// borrows.
unsafe impl<T: Send> Send for ColumnCursorMut<'_, T> {}
// SAFETY: as above.
unsafe impl<T: Sync> Sync for ColumnCursorMut<'_, T> {}
