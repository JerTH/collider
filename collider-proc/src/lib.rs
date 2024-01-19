extern crate proc_macro;
extern crate proc_macro2;
extern crate quote;
extern crate syn;

use std::sync::atomic::AtomicU16;

use proc_macro2::{TokenStream, Ident, Span};

use quote::quote;
use syn::{parse_macro_input, DeriveInput, Field, spanned::Spanned};
use crate::selection::SelectionInput;

mod selection;

static INVOCATION_INIT: u16 = 0xAAAA;
static INVOCATION: AtomicU16 = AtomicU16::new(INVOCATION_INIT);

fn invocation_count() -> u16 {
    INVOCATION.fetch_add(1u16, std::sync::atomic::Ordering::SeqCst)
}

#[proc_macro_attribute]
pub fn component(_meta: proc_macro::TokenStream, input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let _invocation_count = invocation_count();
    let _invocation_count_formatted = format!("{:X}", _invocation_count);
    let _invocation_string: &str = _invocation_count_formatted.as_str();

    let input2: TokenStream = input.clone().into();
    let ast = parse_macro_input!(input as DeriveInput);
    
    let _visibility = ast.vis;
    let _name = ast.ident;
    let _generics = ast.generics;
    
    let out = quote! {
        #[derive(Debug, Default, Clone)]
        #input2
        
        impl collider_core::component::Component for #_name {}
    };

    proc_macro::TokenStream::from(out)
}

#[proc_macro_attribute]
pub fn selection(_meta: proc_macro::TokenStream, input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let _invocation_count = invocation_count();
    let _invocation_count_formatted = format!("{:X}", _invocation_count);
    let _invocation_string: &str = _invocation_count_formatted.as_str();
    
    let input2 = input.clone();

    let _selection = parse_macro_input!(input2 as SelectionInput);
    let _name = _selection.name;
    let _name_ext_span = _name.span();
    let _name_ext_string = format!("{}{}", _name.to_string(), _invocation_string);
    let _name_ext = Ident::new(_name_ext_string.as_str(), _name_ext_span);
    
    let _primary_names_clone = _selection.primary_fields.clone();
    let _primary_access_clone = _selection.primary_fields.clone();
    let _primary_component_clone = _selection.primary_fields.clone();
    let _primary_names = _primary_names_clone.iter().map(|f| {
        let _primary_name_span = Span::mixed_site();
        let _primary_name_string = format!("__{}", f.name().to_string());
        Ident::new(&_primary_name_string.as_str(), _primary_name_span)
    });
    let _primary_access = _primary_access_clone.iter().map(|f| f.access());
    let _primary_component = _primary_component_clone.iter().map(|f| f.component());

    let _secondary_fields_selection_clone = _selection.secondary_fields.clone();
    let _secondary_fields = _secondary_fields_selection_clone.iter().map(|f| {
        let _field = f.field();
        let _field_name_span = _field.ident.span();
        let _field_name_string = format!("__{}", _field.ident.expect("expected a named field").to_string());
        Field {
            attrs: _field.attrs,
            vis: _field.vis,
            mutability: _field.mutability,
            ident: Some(Ident::new(&_field_name_string, _field_name_span)),
            colon_token: _field.colon_token,
            ty: _field.ty,
        }
    });

    let _secondary_fields_reads_types = _secondary_fields.clone().map(|f| f.ty);
    let _secondary_fields_writes_types = _secondary_fields.clone().map(|f| f.ty);

    let _secondary_field_getters_clone = _selection.secondary_fields.clone();
    let _secondary_field_getter_names = _secondary_field_getters_clone.iter().map(|f| {
        f.field().ident.expect("expected a named field")
    });
    let _secondary_field_getter_types = _secondary_field_getters_clone.iter().map(|f| {
        f.field().ty
    });
    
    let _primary_reads_clone = _selection.primary_fields.clone();
    let _primary_reads = _primary_reads_clone.iter().filter_map(|f| match f {
        selection::PrimarySelectionField::Read { component, .. } => Some(component),
        _ => None,
    });

    let _primary_writes_clone = _selection.primary_fields.clone();
    let _primary_writes = _primary_reads_clone.iter().filter_map(|f| match f {
        selection::PrimarySelectionField::Write{ component, .. } => Some(component),
        _ => None,
    });
    
    let _iter_name_span = Span::mixed_site();
    let _iter_name_string = format!("{}Iter", _name.to_string());
    let _iter_name = Ident::new(&_iter_name_string.as_str(), _iter_name_span);
    
    let _iter_item_name_span = Span::mixed_site();
    let _iter_item_name_string = format!("{}IterItem", _name.to_string());
    let _iter_item_name = Ident::new(&_iter_item_name_string.as_str(), _iter_item_name_span);

    let _iter_primary_field_names = _primary_names.clone();
    let _iter_primary_field_access = _primary_access.clone();
    let _iter_primary_field_types = _primary_component.clone();
    let _iter_secondary_field_names = _secondary_fields.clone().filter_map(|f| f.ident);
    let _iter_secondary_field_types = _secondary_fields.clone().map(|f| f.ty);
    
    let out = quote! {
        #[derive(Debug)]
        pub struct #_name<'db> {
            //#(#_field_names: #_field_types,)*
            __generated_db_ref: &'db EntityDatabase,
            __generated_marker: std::marker::PhantomData<&'db ()>,
            #(#_primary_names: #_primary_access<#_primary_component>,)*
            #(#_secondary_fields,)*
        }

        impl<'db> #_name<'db> {
            #(
                pub fn #_secondary_field_getter_names(&'db self)
                    -> <#_secondary_field_getter_types as collider_core::select::DerefSelectionField<'db>>::Ref 
                {
                    todo!()
                }
            )*
        }

        impl<'db> collider_core::select::Selects for #_name<'db> {
            const READS: &'static [collider_core::component::ComponentType] = &[
                #(ComponentType::of::<#_primary_reads>(),)*
            ];
            
            const WRITES: &'static [collider_core::component::ComponentType] = &[
                #(ComponentType::of::<#_primary_writes>(),)*
            ];

            const GLOBAL: &'static [collider_core::component::ComponentType] = &[];

            fn reads() -> Vec<collider_core::component::ComponentType> {
                [Self::READS, Self::GLOBAL]
                    .concat().into_iter()
                    #(.chain(<#_secondary_fields_reads_types as collider_core::select::Selects>::reads()))*
                    .collect()
            }

            fn writes() -> Vec<collider_core::component::ComponentType> {
                Self::WRITES.to_vec().into_iter()
                    #(.chain(<#_secondary_fields_writes_types as collider_core::select::Selects>::writes()))*
                    .collect()
            }
        }
        
        impl<'db> collider_core::select::DatabaseSelection for #_name<'db> {
            fn run_transformation(db: &impl collider_core::database::EntityDatabase) -> TransformationResult {
                use collider_core::component::ComponentTypeSet;
                use collider_core::select::Selects;
                use collider_core::id::FamilyId;
                use collider_core::id::FamilyIdSet;

                let reads = <Self as Selects>::reads();
                let writes = <Self as Selects>::writes();
                
                let row_components: Vec<ComponentType> = reads.iter().chain(writes.iter()).cloned().collect();
                let component_set: ComponentTypeSet = ComponentTypeSet::from(row_components);

                let matching_families: Vec<FamilyId> = db
                    .query_mapping::<ComponentTypeSet, FamilyIdSet>(&component_set)
                    .expect("expected matching families")
                    .clone_into_vec();

                let column_keys = matching_families
                    .iter()
                    .map(|family| {
                        db.get_table::<  todo!()  >(family).expect("expected table for family")
                    })
                    .map(|table| {
                        row_components.iter().map(|component| {
                            table.column_map().get(component).expect("expected column key")
                        })
                    })
                    .flatten();
                
                Ok(())
            }
        }
        
        pub struct #_iter_item_name<'db> {
            __generated_marker: std::marker::PhantomData<&'db ()>,
            #(#_iter_primary_field_names: <#_iter_primary_field_access<#_iter_primary_field_types> as collider_core::select::DerefSelectionField<'db>>::Ref,)*
            #(#_iter_secondary_field_names: <#_iter_secondary_field_types as collider_core::select::DerefSelectionField<'db>>::Ref,)*
        }
        
        impl<'db> IntoIterator for &#_name<'db> {
            type IntoIter = #_iter_name<'db>;
            type Item = #_iter_item_name<'db>;

            fn into_iter(self) -> Self::IntoIter {
                todo!()
            }
        }
        
        impl<'db> IntoIterator for #_name<'db> {
            type IntoIter = #_iter_name<'db>;
            type Item = #_iter_item_name<'db>;

            fn into_iter(self) -> Self::IntoIter {
                todo!()
            }
        }

        pub struct #_iter_name<'db> {
            __generated_db_ref: &'db EntityDatabase,
            __generated_row_width: usize,
            __generated_table_index: usize,
            __generated_column_index: usize,
            __generated_key_chain: Vec<collider_core::id::ColumnKey>,
        }

        impl<'db> Iterator for #_iter_name<'db> {
            type Item = #_iter_item_name<'db>;

            fn next(&mut self) -> Option<Self::Item> {
                todo!()
            }
        }
    };

    proc_macro::TokenStream::from(out)
}
