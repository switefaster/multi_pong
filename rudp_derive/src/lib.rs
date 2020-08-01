use proc_macro::TokenStream;
use syn::DeriveInput;

#[proc_macro_derive(PacketDesc, attributes(packet))]
pub fn packet_desc_derive(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    expand_desc_input(&input);
}

fn expand_desc_input(derive_input: &DeriveInput) -> TokenStream {
    if let syn::Data::Enum(_) = derive_input.data {
        unimplemented!()
    } else {
        panic!("This derive macro is designed only for enum type!");
    }
}
