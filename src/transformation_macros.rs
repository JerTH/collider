//! Transformation Macros
//!
//! TODO: Make all of the below less awful. Procedural macros?
//!
//! These macros make it possible to perform ergonomic
//! selections of data in an [EntityDatabase]


#[macro_use]
pub mod macros {
    #[macro_export]
    macro_rules! impl_transformations {
        ($([$t:ident, $i:tt]),*) => {
            #[allow(unused_parens)]
            impl<'db, $($t),+> Iterator for crate::transform::RowIter<'db, ($($t),+)>
            where
                $(
                    $t: crate::transform::MetaData,
                    $t: crate::transform::SelectOne<'db>,
                    <$t as crate::transform::SelectOne<'db>>::Type: crate::components::Component,
                    *mut Vec<$t::Type>: crate::database::reckoning::GetAsRefType<'db, $t, <$t as crate::transform::SelectOne<'db>>::Ref>,
                )+
            {
                type Item = ($($t::Ref,)+);
                fn next(&mut self) -> Option<Self::Item> {
                    if self.table_index >= (self.keys.len() / self.width) {
                        return None;
                    }
                    use crate::database::reckoning::GetAsRefType;
                    let row: Self::Item = (
                        $(
                            unsafe {
                                // Here we take a reference to our borrow and an opaque pointer to the column we're interested in
                                // and through various type system gymnastics we transform the pointer into an accessor with the
                                // correct mutablity, try to get a component from the column, and check for out of bounds
                                let (_, pointer): &(crate::borrowed::RawBorrow, std::ptr::NonNull<std::os::raw::c_void>) = self.borrows.get_unchecked(self.table_index + $i);
                                let casted: std::ptr::NonNull<Vec<$t::Type>> = pointer.cast::<Vec<$t::Type>>();
                                let raw: *mut Vec<$t::Type> = casted.as_ptr();
                                if let Some(result) = <*mut Vec<$t::Type> as GetAsRefType<'db, $t, <$t as crate::transform::SelectOne<'db>>::Ref>>::get_as_ref_type(&raw, self.column_index) {
                                    result
                                } else {
                                    self.table_index += 1;
                                    self.column_index = 0;
                                    return self.next()
                                }
                            }
                        ,)+
                    );
                    self.column_index += 1;
                    Some(row)
                }
            }
            #[allow(unused_parens)]
            impl<'db, $($t),+> IntoIterator for crate::transform::Rows<'db, ($($t),+)>
            where
                $(
                    $t: crate::transform::MetaData,
                    $t: crate::transform::SelectOne<'db>,
                    <$t as crate::transform::SelectOne<'db>>::Type: crate::components::Component,
                    <$t as crate::transform::SelectOne<'db>>::BorrowType: crate::column::BorrowAsRawParts,
                    crate::column::Column: crate::column::BorrowColumnAs<<$t as crate::transform::SelectOne<'db>>::Type, <$t as crate::transform::SelectOne<'db>>::BorrowType>,
                    crate::column::Column: crate::column::MarkIfWrite<<$t as crate::transform::SelectOne<'db>>::BorrowType>,
                    *mut Vec<$t::Type>: crate::database::reckoning::GetAsRefType<'db, $t, <$t as crate::transform::SelectOne<'db>>::Ref>,
                    $t: 'static,
                )+
            {
                type Item = ($($t::Ref,)+);
                type IntoIter = crate::transform::RowIter<'db, ($($t),+)>;
                fn into_iter(self) -> Self::IntoIter {
                    (&self).into_iter()
                }
            }
            #[allow(unused_parens)]
            impl<'db, $($t),+> IntoIterator for &crate::transform::Rows<'db, ($($t),+)>
            where
                $(
                    $t: crate::transform::MetaData,
                    $t: crate::transform::SelectOne<'db>,
                    <$t as crate::transform::SelectOne<'db>>::Type: crate::components::Component,
                    <$t as crate::transform::SelectOne<'db>>::BorrowType: crate::column::BorrowAsRawParts,
                    *mut Vec<$t::Type>: crate::database::reckoning::GetAsRefType<'db, $t, <$t as crate::transform::SelectOne<'db>>::Ref>,
                    crate::column::Column: crate::column::BorrowColumnAs<<$t as crate::transform::SelectOne<'db>>::Type, <$t as crate::transform::SelectOne<'db>>::BorrowType>,
                    crate::column::Column: crate::column::MarkIfWrite<<$t as crate::transform::SelectOne<'db>>::BorrowType>,
                    $t: 'static,
                )+
            {
                type Item = ($($t::Ref,)+);
                type IntoIter = crate::transform::RowIter<'db, ($($t),+)>;
                fn into_iter(self) -> Self::IntoIter {
                    let db = self.database();
                    let mut iter = crate::transform::RowIter::<'db, ($($t),+)>::new(db);
                    for i in 0..(self.keys.len() / self.width) {
                        let borrows: ($($t::BorrowType,)+) = ($(
                            unsafe {
                                use crate::column::BorrowColumnAs;
                                let col_idx = (i * self.width) + $i;
                                let column = db.get_column(self.keys.get_unchecked(col_idx)).expect("expected initialized column for iteration");
                                <crate::column::Column as crate::column::MarkIfWrite<<$t as crate::transform::SelectOne<'db>>::BorrowType>>::mark_if_write(&column);
                                <$t as crate::transform::SelectOne<'db>>::BorrowType::from(column.borrow_column_as())
                            }
                        ,)+);
                        $(
                            unsafe {
                                use crate::column::BorrowAsRawParts;
                                iter.borrows.push((borrows.$i).borrow_as_raw_parts());
                            }
                        )+
                    }
                    iter.keys = self.keys.clone();
                    iter.width = self.width;
                    iter
                }
            }
            #[allow(unused_parens)]
            impl<'a, $($t),+> const crate::transform::Selection for ($($t),+)
            where
                $(
                    $t: crate::transform::MetaData,
                    $t: ~const crate::transform::SelectOne<'a>,
                )+
            {
                const READS: &'static [Option<crate::components::ComponentType>] = &[$($t::reads(),)+];
                const WRITES: &'static [Option<crate::components::ComponentType>] = &[$($t::writes(),)+];
            }
        };
    }
}

impl_transformations!([A, 0]);
impl_transformations!([A, 0], [B, 1]);
impl_transformations!([A, 0], [B, 1], [C, 2]);
impl_transformations!([A, 0], [B, 1], [C, 2], [D, 3]);
impl_transformations!([A, 0], [B, 1], [C, 2], [D, 3], [E, 4]);
impl_transformations!([A, 0], [B, 1], [C, 2], [D, 3], [E, 4], [F, 5]);
impl_transformations!([A, 0], [B, 1], [C, 2], [D, 3], [E, 4], [F, 5], [G, 6]);
impl_transformations!([A, 0], [B, 1], [C, 2], [D, 3], [E, 4], [F, 5], [G, 6], [H, 7]);
