//! The syntax of `scn!` and `scn_list!`.
//!
//! A scene is written as a sequence of entries, and an entry is one of:
//!
//! ```text
//! #Name                              // name this entity, so the scene can point at it
//! Type { field: value, ... }         // describe a component through its template
//! Type(args)                         // describe a component through a call
//! ~template { field: value, ... }    // the same, for a hand-written template
//! ~template(args)
//! ~expression                        // any value the scene stores as a canonical template
//! Path                               // make sure the canonical template of `Path` exists
//! Children [ entry* (-- entry*)* ]   // the entities under this one
//! Parent(value)                      // an explicit parent edge for this entity
//! : expression                       // a cached scene asset this scene builds on
//! @ expression                       // a scene included as it is, without caching
//! ```
//!
//! `scn!` describes one entity; `scn_list!` describes several, separated by `--`, which is also what
//! separates the children in a `Children [ ... ]` list.
//!
//! A value is passed through as it is written, except for a lone `#Name`, which becomes the entity
//! that name stands for.
//!
//! # Cached scenes
//!
//! `: expression` names a scene asset — an expression that becomes an `AssetPath` — and makes the
//! scene build on it. The cached scene is applied first, and a template the scene describes that the
//! cached one also describes is *cloned out of it* rather than created from scratch, which is what
//! lets a description patch a template it does not own.
//!
//! That only works if the cached scene is there before the rest of the composition, so a `:` entry
//! has to come before every entry that describes a template or a child — names may precede it, since
//! they describe neither.
//!
//! `@ expression` includes a scene without caching: the expression is a `Scene` — whatever it is —
//! and it is resolved exactly where it is written, in entry order.

use proc_macro2::TokenStream;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Expr, Ident, Index, Member, Path, Token, braced, bracketed};

// The separator between the entities of a list.
syn::custom_punctuation!(DashDash, --);

// -----------------------------------------------------------------------------
// Roots

/// The body of `scn!`: one entity.
pub struct SceneRoot(pub Scene);

/// The body of `scn_list!`: a list of entities.
pub struct SceneListRoot(pub SceneList);

impl Parse for SceneRoot {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self(input.parse()?))
    }
}

impl Parse for SceneListRoot {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self(input.parse()?))
    }
}

// -----------------------------------------------------------------------------
// Scenes

/// One entity: the entries that describe it.
pub struct Scene {
    /// The entries, in the order they were written.
    pub entries: Vec<Entry>,
}

impl Parse for Scene {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut entries: Vec<Entry> = Vec::new();
        while !input.is_empty() && !input.peek(DashDash) && !input.peek(Token![,]) {
            let span = input.span();
            let entry: Entry = input.parse()?;

            // A cached scene is applied before everything else, so the entries that edit the
            // templates it contributes have to come after it. Names are exempt: they describe no
            // template and no child.
            if matches!(entry, Entry::CachedScene(_))
                && entries.iter().any(|entry| !matches!(entry, Entry::Name(_)))
            {
                return Err(syn::Error::new(
                    span,
                    "a cached scene has to come before every entry that describes a template or a \
                     child: it is applied first, and the entries after it edit what it contributes. \
                     Move it to the front, or write `@` instead of `:` to include it where it is.",
                ));
            }

            entries.push(entry);
        }
        Ok(Self { entries })
    }
}

/// Several entities: the scenes of a list, in order.
pub struct SceneList {
    /// One scene per entity.
    pub scenes: Vec<Scene>,
}

impl Parse for SceneList {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut scenes = Vec::new();
        loop {
            scenes.push(input.parse()?);
            if input.peek(DashDash) {
                input.parse::<DashDash>()?;
            } else {
                break;
            }
        }
        Ok(Self { scenes })
    }
}

// -----------------------------------------------------------------------------
// Entries

/// One entry of a scene.
pub enum Entry {
    /// `#Name` — the name this entity is known by.
    Name(Ident),

    /// `Children [ ... ]` — the entities under this one.
    Children(SceneList),

    /// `Parent(value)` — an explicit parent edge.
    Parent(Value),

    /// A template value inserted as it is: `~expression`.
    TemplateValue(TokenStream),

    /// `<Type> { field: value, ... }`, or the same behind a `~` — the canonical template, edited in
    /// place.
    Patch {
        /// The template type: `Type` or `<Type as FromTemplate>::Template`.
        template: TemplateType,
        /// The fields to assign.
        fields: Vec<Field>,
    },

    /// `<Type>(args)`, or the same behind a `~` — a call that produces the template.
    Constructor {
        /// The type the call belongs to, without the function segment.
        template: TemplateType,
        /// The function that is called on it.
        function: Ident,
        /// The arguments of the call.
        args: TokenStream,
    },

    /// `<Type>` — the canonical template of the type, created if it is not there yet.
    Init(TemplateType),

    /// `: expression` — a cached scene asset this scene builds on.
    CachedScene(Expr),

    /// `@ expression` — a scene included as it is, without caching.
    Scene(Expr),
}

/// The type of a template: the type itself, or the one it is built from.
pub struct TemplateType {
    /// The type as it was written.
    pub path: Path,
    /// Whether `~` was written, which means the path *is* the template.
    pub raw: bool,
}

/// One field assignment of a patch.
pub struct Field {
    /// The field: a name, or the position of a tuple field.
    pub member: Member,
    /// The value it is assigned.
    pub value: Value,
}

/// A value: everything the author wrote, except for a name.
pub enum Value {
    /// `#Name` — the entity that name stands for.
    Name(Ident),
    /// Anything else, passed through.
    Tokens(TokenStream),
}

impl Parse for Entry {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // `: expression` — the cached scene this one builds on. The `::` of a leading path segment
        // is not this.
        if input.peek(Token![:]) && !input.peek(Token![::]) {
            input.parse::<Token![:]>()?;
            return Ok(Self::CachedScene(input.parse()?));
        }

        // `@ expression` — a scene included where it is written.
        if input.peek(Token![@]) {
            input.parse::<Token![@]>()?;
            return Ok(Self::Scene(input.parse()?));
        }

        // `#Name`
        if input.peek(Token![#]) {
            input.parse::<Token![#]>()?;
            return Ok(Self::Name(input.parse()?));
        }

        // `~expression` — a value that stands for a template as it is.
        let raw = input.peek(Token![~]);
        if raw {
            input.parse::<Token![~]>()?;
            if input.peek(syn::token::Brace) {
                let content;
                braced!(content in input);
                let tokens: TokenStream = content.parse()?;
                return Ok(Self::TemplateValue(tokens));
            }
        }

        let path: Path = input.parse()?;
        let ident = path.segments.last().map(|segment| &segment.ident);

        // `Children [ ... ]`
        if !raw && ident.is_some_and(|ident| ident == "Children") && input.peek(syn::token::Bracket)
        {
            let content;
            bracketed!(content in input);
            return Ok(Self::Children(content.parse()?));
        }

        // `Parent(value)`
        if !raw && ident.is_some_and(|ident| ident == "Parent") && input.peek(syn::token::Paren) {
            let content;
            syn::parenthesized!(content in input);
            return Ok(Self::Parent(content.parse()?));
        }

        // `<Type> { ... }`
        if input.peek(syn::token::Brace) {
            let content;
            braced!(content in input);
            let mut fields = Vec::new();
            while !content.is_empty() {
                let name: Ident = content.parse()?;
                content.parse::<Token![:]>()?;
                let value: Value = content.parse()?;
                fields.push(Field {
                    member: Member::Named(name),
                    value,
                });
                if content.peek(Token![,]) {
                    content.parse::<Token![,]>()?;
                }
            }
            return Ok(Self::Patch {
                template: TemplateType { path, raw },
                fields,
            });
        }

        // `<Type>(args)` — a call on the type, or the fields of a tuple struct.
        if input.peek(syn::token::Paren) {
            let content;
            syn::parenthesized!(content in input);
            let last = path.segments.last().map(|segment| segment.ident.clone());
            let is_function = path.segments.len() > 1
                && last
                    .as_ref()
                    .is_some_and(|ident| ident.to_string().starts_with(char::is_lowercase));

            if is_function {
                let function = last.expect("the path has a last segment");

                // Dropping the function leaves the separator of the segment before it behind — a
                // path is parsed as (segment, separator) pairs plus a last segment — and `Type::` is
                // not a type. Collecting the segments again puts the separators back between them.
                let leading_colon = path.leading_colon;
                let mut segments = path.segments;
                segments.pop();
                let segments = segments.into_iter().collect();

                return Ok(Self::Constructor {
                    template: TemplateType {
                        path: Path {
                            leading_colon,
                            segments,
                        },
                        raw,
                    },
                    function,
                    args: content.parse()?,
                });
            }

            let values = Punctuated::<Value, Token![,]>::parse_terminated(&content)?;
            let fields = values
                .into_iter()
                .enumerate()
                .map(|(index, value)| Field {
                    member: Member::Unnamed(Index::from(index)),
                    value,
                })
                .collect();

            return Ok(Self::Patch {
                template: TemplateType { path, raw },
                fields,
            });
        }

        Ok(Self::Init(TemplateType { path, raw }))
    }
}

impl Parse for Value {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(Token![#]) {
            let forked = input.fork();
            forked.parse::<Token![#]>()?;
            if forked.parse::<Ident>().is_ok()
                && (forked.is_empty()
                    || forked.peek(Token![,])
                    || forked.peek(Token![;])
                    || forked.peek(syn::token::Paren))
            {
                input.parse::<Token![#]>()?;
                return Ok(Self::Name(input.parse()?));
            }
        }

        // Everything else is passed through, up to the comma (or the end) that closes the value.
        let mut tokens = TokenStream::new();
        while !input.is_empty() && !input.peek(Token![,]) {
            let tree: proc_macro2::TokenTree = input.parse()?;
            tokens.extend([tree]);
        }
        Ok(Self::Tokens(tokens))
    }
}
