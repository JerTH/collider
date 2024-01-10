use syn::{Ident, parse::{Parse, ParseStream, Result}, Token, token, Field, braced, Attribute};


#[derive(Debug)]
pub struct SelectionInput {
    pub attrs: Vec<Attribute>,
    pub struct_token: Token![struct],
    pub name: Ident,
    pub brace_token: token::Brace,
    pub primary_fields: Vec<PrimarySelectionField>,
    pub secondary_fields: Vec<SecondarySelectionField>,
}

#[derive(Debug, Clone)]
pub enum PrimarySelectionField {
    Read {
        name: Ident,
        access: Ident,
        component: Ident,
    },
    Write {
        name: Ident,
        access: Ident,
        component: Ident,
    }
}

impl PrimarySelectionField {
    pub fn name(&self) -> Ident {
        match self {
            PrimarySelectionField::Read { name, .. } => name.clone(),
            PrimarySelectionField::Write { name, .. } => name.clone(),
        }
    }

    pub fn access(&self) -> Ident {
        match self {
            PrimarySelectionField::Read { access, .. } => access.clone(),
            PrimarySelectionField::Write { access, .. } => access.clone(),
        }
    }

    pub fn component(&self) -> Ident {
        match self {
            PrimarySelectionField::Read { component, .. } => component.clone(),
            PrimarySelectionField::Write { component, .. } => component.clone(),
        }
    }
}

impl Parse for PrimarySelectionField {
    fn parse(input: ParseStream) -> Result<Self> {
        eprintln!("parse psf");
        let _name: Ident = input.parse()?;
        eprintln!("parsed ident");
        let _colon: Token![:] = input.parse()?;
        eprintln!("parsed colon");
        let _access: Ident = input.parse()?;
        eprintln!("parsed access");

        let _lbracket: Token![<] = input.parse()?;
        let _component: Ident = input.parse()?;

        let _rbracket: Token![>] = input.parse()?;
        if input.peek(Token![,]) {
            let _comma: Token![,] = input.parse()?;
        }
        
        match _access.to_string().as_str() {
            "Write" => {
                Ok(PrimarySelectionField::Write {
                    name: _name,
                    access: _access,
                    component: _component
                })
            },
            "Read" => {
                Ok(PrimarySelectionField::Read {
                    name: _name,
                    access: _access,
                    component: _component
                })
            },
            _ => {
                return Err(input.error("not a primary field"));
            }
        }
    }
}

fn peek_is_primary_field(input: ParseStream) -> bool {
    let token_tree = input.cursor().token_tree();

    if token_tree.is_none() {
        return false
    }

    let mut cursor = input.cursor();
    if let Some((_ident, c)) = cursor.ident() {
        cursor = c;
        if let Some((_punct, c)) = cursor.punct() {
            cursor = c;
            if let Some((_access, _c)) = cursor.ident() {
                match _access.to_string().as_str() {
                    "Write" | "Read" => {
                        return true;
                    },
                    _ => {}
                } 
            }
        }
    }
    return false
}

impl Parse for SelectionInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let contents;
        let mut out = SelectionInput {
            attrs: input.call(Attribute::parse_outer)?,
            struct_token: input.parse()?,
            name: input.parse()?,
            brace_token: braced!(contents in input),
            primary_fields: Vec::new(),
            secondary_fields: Vec::new(),
        };

        let mut primary_fields: Vec<PrimarySelectionField> = Vec::new();
        while peek_is_primary_field(&contents) {
            eprintln!("GOT A PRIMARY FIELD");
            let primary: PrimarySelectionField = contents.parse()?;
            primary_fields.push(primary);
        }

        let parsed_secondaries = contents.parse_terminated(Field::parse_named, Token![,])?;
        
        let mut secondary_fields: Vec<SecondarySelectionField> = Vec::new();
        for field in parsed_secondaries {
            let secondary: SecondarySelectionField = SecondarySelectionField { field };
            secondary_fields.push(secondary);
        }

        out.primary_fields = primary_fields;
        out.secondary_fields = secondary_fields;
        Ok(out)
    }
}

#[derive(Debug, Clone)]
pub struct SecondarySelectionField {
    field: Field
}

impl SecondarySelectionField {
    pub fn field(&self) -> Field {
        self.field.clone()
    }
}
