pub(crate) mod api {

    use std::collections::HashSet;

    use proc_macro2::{Span, TokenStream};
    use quote::{quote, ToTokens};
    use syn::{
        parenthesized, punctuated::Punctuated, token::RArrow, Expr, Ident, ItemMod, ItemTrait,
        LitByteStr, Meta, PatType, Path, Signature, Token, TraitItem, UsePath,
    };

    pub static EXPORT_FN_HASH: &str =
        "66a798624914b7174826b51e4baeb73b9695d3f333eac18069b81cf51d5029e4";

    pub fn camel_to_snake_case(s: &str) -> String {
        let mut result = String::new();
        for (i, c) in s.chars().enumerate() {
            if c.is_uppercase() && i > 0 {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
        }
        result
    }

    fn generate_dis_fn(
        trait_: &Path,
        enum_name: &Ident,
        sig: &Signature,
        vars: &[Ident],
    ) -> syn::Result<proc_macro2::TokenStream> {
        let ident = &sig.ident;

        let args: Vec<_> = sig
            .inputs
            .iter()
            .filter_map(|r| match r {
                syn::FnArg::Receiver(_) => None,
                syn::FnArg::Typed(PatType { pat, .. }) => Some(quote! { #pat }),
            })
            .collect();

        let mut future_output = None;
        if let syn::ReturnType::Type(_, ty) = &sig.output {
            if let syn::Type::ImplTrait(impl_trait) = &**ty {
                for bound in &impl_trait.bounds {
                    let syn::TypeParamBound::Trait(ty) = bound else {
                        continue;
                    };
                    let path_seg = ty.path.segments.first().unwrap().clone();
                    if path_seg.ident != "Future" {
                        continue;
                    }
                    let syn::PathArguments::AngleBracketed(arg) = path_seg.arguments else {
                        continue;
                    };
                    for ele in arg.args {
                        let syn::GenericArgument::AssocType(ty) = ele else {
                            continue;
                        };
                        if ty.ident == "Output" {
                            future_output = Some(ty.ty);
                        }
                    }
                }
            }
        }

        let handle_tokens = if sig.asyncness.is_some() || future_output.is_some() {
            quote! {
                #trait_::#ident(plug, #(#args),* ).await
            }
        } else {
            quote! {
                #trait_::#ident(plug, #(#args),* )
            }
        };

        let arm_tokens = quote! {
            #(#enum_name::#vars(plug) => #handle_tokens,)*
            #enum_name::DynPlugin(plug) => #handle_tokens,
        };
        let sig = if let Some(out_ty) = future_output {
            let mut new_sig = sig.clone();
            new_sig.output = syn::ReturnType::Type(RArrow::default(), Box::new(out_ty));
            new_sig.asyncness = Some(Default::default());
            new_sig
        } else {
            sig.clone()
        };

        Ok(quote! {
                #sig {
                    match self {
                        #arm_tokens
                    }
                }
        })
    }

    fn make_macro(
        trait_data: &ItemTrait,
        dyn_ty: Option<Path>,
        inner_tt: &TokenStream,
    ) -> syn::Result<proc_macro2::TokenStream> {
        let trait_name = &trait_data.ident;
        let name_snake = camel_to_snake_case(&trait_name.to_string());
        let funcs = trait_data.items.iter().filter_map(|r| {
            if let TraitItem::Fn(func) = r {
                Some(func.sig.clone())
            } else {
                None
            }
        });

        let dyn_ty = dyn_ty.unwrap_or_else(|| trait_name.clone().into());
        let dyn_ty_plugin = quote! { $crate::#dyn_ty };

        let call_site = Span::call_site();
        let cmptime_fn_ident = Ident::new(&format!("__make_cmptime_{name_snake}"), call_site);
        let dyn_fn_ident = Ident::new(
            &format!("__{EXPORT_FN_HASH}_make_dyn_{name_snake}"),
            call_site,
        );
        let export_plug_macro = quote! {
            /// Export plugin struct.
            #[macro_export]
            macro_rules! export_plugin {
                ($plug:ty) => {
                    #[doc(hidden)]
                    pub fn #cmptime_fn_ident() -> $plug {
                        <$plug as Default>::default()
                    }

                    #[doc(hidden)]
                    #[no_mangle]
                    pub extern "Rust" fn #dyn_fn_ident() -> #dyn_ty_plugin {
                        #dyn_ty_plugin::new(<$plug as Default>::default())
                    }

                    #[doc(hidden)]
                    pub type __ExportedPlugin = $plug;
                };
            }

        };

        let static_name_dyn = LitByteStr::new(dyn_fn_ident.to_string().as_bytes(), call_site);
        let dispatcher_macro_name =
            Ident::new(&format!("define_dispatcher_{name_snake}"), call_site);
        let load_plugin_name = Ident::new(&format!("load_cmptime_{name_snake}"), call_site);

        Ok(quote! {
            /// Static name for the dynamic plugin loader function.
            pub static DYN_LOADER_FN_NAME: &'static [u8] = #static_name_dyn;
            /// Dynamic plugin loader entry.
            pub type DynPluginLoader = extern "Rust" fn() -> #dyn_ty;

            pub use carolina_api_macros::__generate_enum;

            /// Generated macro for plugin system to create static dispatching enum.
            /// **DO NOT** use this in **PLUGIN** environment!
            #[macro_export]
            macro_rules! #dispatcher_macro_name {
                ($vis:vis $e_name:ident( $($plug_crate:ident),* )) => {

                    #[doc(hidden)]
                    mod __plugin_dispatcher {
                        use super::*;
                        use #dyn_ty_plugin as DynTy;
                        use $crate::#trait_name as Trait;
                        #inner_tt

                        $crate::__generate_enum!(
                            $vis $e_name Trait DynTy (
                                $($plug_crate),*
                            ) ( #(#funcs);* )
                        );
                    }

                    pub use __plugin_dispatcher::*;
                };
            }

            #[macro_export]
            /// Generated macro for loading compile-time plugins.
            /// **DO NOT** use this in **PLUGIN** environment!
            macro_rules! #load_plugin_name {
                ($export_path:ident) => {{
                    $export_path::#cmptime_fn_ident()
                }};
            }

            #export_plug_macro
        })
    }

    /// Extrat module, return trait, other module inner tokens, and tokens for macro inner.
    fn extract_mod(module: &ItemMod) -> syn::Result<(ItemTrait, TokenStream)> {
        use syn::Item;

        let Some((_, items)) = module.content.as_ref() else {
            return Err(syn::Error::new_spanned(module, "module is empty"));
        };

        let mut tt = TokenStream::new();
        let mut target = None;
        for ele in items {
            match ele {
                Item::Trait(item_trait) => {
                    if target.is_some() {
                        return Err(syn::Error::new_spanned(ele, "more than one trait received"));
                    }
                    target = Some(item_trait);
                }
                Item::Use(item) => {
                    let mut item = item.clone();
                    if let syn::UseTree::Path(UsePath {
                        ident,
                        colon2_token: _,
                        tree,
                    }) = &mut item.tree
                    {
                        let tokens = if ident == "crate" {
                            quote! { use $crate::#tree; }
                        } else {
                            quote! { use #ident::#tree; }
                        };
                        tokens.to_tokens(&mut tt);
                    } else {
                        item.to_tokens(&mut tt);
                    };
                }
                tokens => tokens.to_tokens(&mut tt),
            }
        }

        let Some(target) = target else {
            return Err(syn::Error::new_spanned(module, "missing trait"));
        };

        Ok((target.clone(), tt))
    }

    struct EnumGen {
        vis: syn::Visibility,
        name: Ident,
        trait_: Path,
        dyn_ty: Path,
        items: Vec<Ident>,
        funcs: Vec<syn::Signature>,
    }
    impl syn::parse::Parse for EnumGen {
        fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
            let vis = input.parse()?;
            let name = input.parse()?;
            let trait_ = input.parse()?;
            let dyn_ty = input.parse()?;

            let var_tokens;
            parenthesized!(var_tokens in input);
            let func_tokens;
            parenthesized!(func_tokens in input);
            let paths = Punctuated::<Ident, Token![,]>::parse_terminated(&var_tokens)?;
            let funcs = Punctuated::<syn::Signature, Token![;]>::parse_terminated(&func_tokens)?;
            Ok(Self {
                vis,
                name,
                trait_,
                dyn_ty,
                items: paths.into_iter().collect(),
                funcs: funcs.into_iter().collect(),
            })
        }
    }

    pub(crate) fn generate_enum(input: TokenStream) -> syn::Result<TokenStream> {
        let EnumGen {
            vis,
            name,
            trait_,
            dyn_ty,
            items,
            funcs,
        } = syn::parse2(input)?;

        let var_names: Vec<Ident> = items
            .iter()
            .map(|item| Ident::new(&item.to_string().to_uppercase(), Span::call_site()))
            .collect();

        let funcs = funcs
            .into_iter()
            .map(|sig| generate_dis_fn(&trait_, &name, &sig, &var_names))
            .collect::<Result<Vec<_>, _>>()?;

        let expanded = quote! {
             #vis enum #name {
                #(#var_names(::#items::__ExportedPlugin),)*
                DynPlugin(#dyn_ty),
            }

            #(
                impl From<::#items::__ExportedPlugin> for #name {
                    fn from(plug: ::#items::__ExportedPlugin) -> Self {
                        Self::#var_names(plug)
                    }
                }
            )*

            impl From<#dyn_ty> for #name {
                fn from(plug: #dyn_ty) -> Self {
                    Self::DynPlugin(plug)
                }
            }

            impl #trait_ for #name {
                #(#funcs)*
            }
        };

        Ok(expanded)
    }

    pub(crate) fn parse_plugin_mod(
        attrs: Vec<Meta>,
        input: ItemMod,
    ) -> syn::Result<proc_macro2::TokenStream> {
        let mod_name = &input.ident;
        let (trait_, tt) = extract_mod(&input)?;
        let trait_name = &trait_.ident;
        let trait_vis = &trait_.vis;
        let mut ignored = HashSet::<Ident>::new();

        let mut dyn_ty = None::<Path>;
        for ele in attrs {
            match ele {
                Meta::List(meta) => {
                    if meta.path.is_ident("ignore") {
                        let args = meta.parse_args_with(
                            Punctuated::<Ident, syn::Token![,]>::parse_terminated,
                        )?;
                        for ele in args.into_iter() {
                            ignored.insert(ele);
                        }
                    } else {
                        return Err(syn::Error::new_spanned(meta, "unknown attribute"));
                    }
                }
                Meta::NameValue(meta) => {
                    if meta.path.is_ident("dyn_t") {
                        let Expr::Path(ty) = meta.value else {
                            return Err(syn::Error::new_spanned(meta.value, "expected path"));
                        };
                        dyn_ty = Some(ty.path);
                    } else {
                        return Err(syn::Error::new_spanned(meta, "unknown attribute"));
                    }
                }
                _ => return Err(syn::Error::new_spanned(ele, "unknown attribute")),
            }
        }

        let macros = make_macro(&trait_, dyn_ty, &tt)?;

        Ok(quote! {
            #input

            #trait_vis use #mod_name::#trait_name;
            #macros
        })
    }
}
