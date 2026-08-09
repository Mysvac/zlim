use proc_macro2::TokenStream;
use syn::visit::{self, Visit};
use syn::{Generics, Ident, Type, parse_quote};

struct IdentFinder<'a> {
    idents: &'a [Ident],
    found: bool,
}

impl<'a> Visit<'a> for IdentFinder<'a> {
    fn visit_ident(&mut self, ident: &'a Ident) {
        if !self.found {
            self.found = self.idents.contains(ident);
        }
    }

    fn visit_type(&mut self, ty: &'a Type) {
        if !self.found {
            visit::visit_type(self, ty);
        }
    }
}

pub(crate) fn contains_any_idents(ty: &Type, idents: &[Ident]) -> bool {
    let mut finder = IdentFinder {
        idents,
        found: false,
    };
    finder.visit_type(ty);
    finder.found
}

pub(crate) fn field_type_constraint(generics: &mut Generics, ty: &Type, constraint: &TokenStream) {
    generics
        .make_where_clause()
        .predicates
        .push(parse_quote! { #ty: #constraint });
}
