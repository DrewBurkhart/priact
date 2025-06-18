use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream, Result};
use syn::{braced, parenthesized, punctuated::Punctuated, token, Ident, ItemFn, Token, Type};

// Represents one field: `name: Type`
struct FieldDef {
    name: Ident,
    #[allow(dead_code)]
    colon_token: Token![:],
    ty: Type,
}

impl Parse for FieldDef {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(FieldDef {
            name: input.parse()?,
            colon_token: input.parse()?,
            ty: input.parse()?,
        })
    }
}

// Represents one method: `@priority(P) fn foo(&mut self, ...) -> Ret { .. }` or `async fn`
struct MethodDef {
    _at: Token![@],
    _prio_kw: Ident,
    _paren: token::Paren,
    priority: Ident,
    func: ItemFn,
}

impl Parse for MethodDef {
    fn parse(input: ParseStream) -> Result<Self> {
        let _at: Token![@] = input.parse()?;
        let _prio_kw: Ident = input.parse()?;
        if _prio_kw != "priority" {
            return Err(syn::Error::new(_prio_kw.span(), "expected `priority`"));
        }
        let content;
        let _paren = parenthesized!(content in input);
        let priority: Ident = content.parse()?;

        let func: ItemFn = input.parse()?;

        Ok(MethodDef {
            _at,
            _prio_kw,
            _paren,
            priority,
            func,
        })
    }
}

// Top-level parse for define_actor!
struct ActorDef {
    actor_name: Ident,
    fields: Punctuated<FieldDef, Token![,]>,
    _impl_kw: Token![impl],
    msg_name: Ident,
    methods: Vec<MethodDef>,
}

impl Parse for ActorDef {
    fn parse(input: ParseStream) -> Result<Self> {
        let actor_name: Ident = input.parse()?;
        let content;
        braced!(content in input);
        let fields = content.parse_terminated(FieldDef::parse)?;

        let _impl_kw: Token![impl] = input.parse()?;
        let msg_name: Ident = input.parse()?;
        let methods_content;
        braced!(methods_content in input);

        let mut methods = Vec::new();
        while !methods_content.is_empty() {
            methods.push(methods_content.parse::<MethodDef>()?);
        }
        Ok(ActorDef {
            actor_name,
            fields,
            _impl_kw,
            msg_name,
            methods,
        })
    }
}

/// The procedural macro entry point
#[proc_macro]
pub fn define_actor(input: TokenStream) -> TokenStream {
    let ActorDef {
        actor_name,
        fields,
        _impl_kw: _,
        msg_name,
        methods,
    } = syn::parse_macro_input!(input as ActorDef);

    // Struct fields
    let struct_fields = fields.iter().map(|f| {
        let name = &f.name;
        let ty = &f.ty;
        quote! { pub #name: #ty, }
    });

    // Enum variants: always tuple variants (even zero-arg)
    let variants = methods.iter().map(|m| {
        let name = &m.func.sig.ident;
        let args: Vec<_> = m
            .func
            .sig
            .inputs
            .iter()
            .skip(1)
            .filter_map(|arg| {
                if let syn::FnArg::Typed(pat_ty) = arg {
                    Some(&pat_ty.ty)
                } else {
                    None
                }
            })
            .collect();
        quote! { #name( #(#args),* ), }
    });

    // Priority match arms
    let priorities = methods.iter().map(|m| {
        let name = &m.func.sig.ident;
        let prio = &m.priority;
        quote! { #msg_name::#name(..) => Priority::#prio, }
    });

    // handle() match arms: always tuple patterns
    let handle_arms = methods.iter().map(|m| {
        let sig = &m.func.sig;
        let name = &sig.ident;
        let is_async = sig.asyncness.is_some();
        let arg_idents: Vec<_> = sig.inputs.iter().skip(1).filter_map(|arg| {
            if let syn::FnArg::Typed(pat_ty) = arg {
                if let syn::Pat::Ident(pi) = &*pat_ty.pat {
                    Some(&pi.ident)
                } else { None }
            } else { None }
        }).collect();

        if is_async {
            quote! { #msg_name::#name( #(#arg_idents),* ) => { self.#name( #(#arg_idents),* ).await; true }, }
        } else {
            quote! { #msg_name::#name( #(#arg_idents),* ) => { self.#name( #(#arg_idents),* ); true }, }
        }
    });

    // Method implementations: directly use the parsed signature and body
    let method_defs = methods.iter().map(|m| {
        let sig = &m.func.sig;
        let body = &m.func.block;
        quote! {
            // Note: ItemFn's visibility is not used; we force `pub` here.
            pub #sig #body
        }
    });

    let expanded = quote! {
        pub struct #actor_name {
            #(#struct_fields)*
        }

        impl Drop for #actor_name {
            fn drop(&mut self) {
                println!("[{}] Actor instance being dropped.", stringify!(#actor_name));
            }
        }

        pub enum #msg_name {
            #(#variants)*
            Shutdown,
        }

        impl Prioritized for #msg_name {
            fn priority(&self) -> Priority {
                match self {
                    #(#priorities)*
                    #msg_name::Shutdown => Priority::Shutdown,
                }
            }
        }

        #[async_trait::async_trait]
        impl Actor for #actor_name {
            type Msg = #msg_name;
            async fn handle(&mut self, msg: Self::Msg) -> bool {
                match msg {
                    #(#handle_arms)*
                    #msg_name::Shutdown => false,
                }
            }
        }

        #[allow(non_snake_case)]
        impl #actor_name {
            #(#method_defs)*
        }
    };

    TokenStream::from(expanded)
}
