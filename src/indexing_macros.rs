//! Indexing Macros
use crate::EntityId;

#[macro_use]
pub mod macros {
    #[macro_export]
    macro_rules! impl_indexing_transformations {
        (EntityId, $([$t:ident, $i:tt]),*) => {


            #[allow(unused_parens)]
            impl<'db, $($t),+> Iterator for crate::indexing::IndexingRowIter<'db, (EntityId, $($t),+)>
            where
                $(
                    $t: crate::transform::MetaData,
                    $t: crate::transform::SelectOne<'db>,
                    <$t as crate::transform::SelectOne<'db>>::Type: crate::components::Component,
                    *mut Vec<$t::Type>: crate::database::reckoning::GetAsRefType<'db, $t, <$t as crate::transform::SelectOne<'db>>::Ref>,
                )+
            {
                type Item = (EntityId, $($t::Ref,)+);

                fn next(&mut self) -> Option<Self::Item> {
                    todo!()
                }
            }


            


            #[allow(unused_parens)]
            impl<'db, $($t),+> IntoIterator for crate::indexing::IndexingRows<'db, (EntityId, $($t),+)>
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
                type Item = (EntityId, $($t::Ref,)+);
                type IntoIter = crate::indexing::IndexingRowIter<'db, (EntityId, $($t),+)>;

                fn into_iter(self) -> Self::IntoIter {
                    (&self).into_iter()
                }
            }





            #[allow(unused_parens)]
            impl<'db, $($t),+> IntoIterator for &crate::indexing::IndexingRows<'db, (EntityId, $($t),+)>
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
                type Item = (EntityId, $($t::Ref,)+);
                type IntoIter = crate::indexing::IndexingRowIter<'db, (EntityId, $($t),+)>;

                fn into_iter(self) -> Self::IntoIter {
                    todo!()
                }
            }





            #[allow(unused_parens)]
            impl<'a, $($t),+> const crate::transform::Selection for (EntityId, $($t),+)
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
} // macros ========================================================================

impl_indexing_transformations!(EntityId, [A, 0]);
impl_indexing_transformations!(EntityId, [A, 0], [B, 1]);
impl_indexing_transformations!(EntityId, [A, 0], [B, 1], [C, 2]);
impl_indexing_transformations!(EntityId, [A, 0], [B, 1], [C, 2], [D, 3]);
impl_indexing_transformations!(EntityId, [A, 0], [B, 1], [C, 2], [D, 3], [E, 4]);
impl_indexing_transformations!(EntityId, [A, 0], [B, 1], [C, 2], [D, 3], [E, 4], [F, 5]);
impl_indexing_transformations!(EntityId, [A, 0], [B, 1], [C, 2], [D, 3], [E, 4], [F, 5], [G, 6]);
impl_indexing_transformations!(EntityId, [A, 0], [B, 1], [C, 2], [D, 3], [E, 4], [F, 5], [G, 6], [H, 7]);
