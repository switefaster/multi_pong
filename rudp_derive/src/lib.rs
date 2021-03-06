use proc_macro::TokenStream;
use syn::{DeriveInput, NestedMeta, Meta, Ident, DataEnum, Fields};
use quote::quote;

enum FieldType {
    Struct,
    Tuple,
    Flat,
}

struct Packet {
    reliable: bool,
    ordered: bool,
    name: Ident,
    field: FieldType,
}

#[proc_macro_derive(PacketDesc, attributes(packet))]
pub fn packet_desc_derive(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    expand_desc_input(input).into()
}

fn expand_desc_input(derive_input: DeriveInput) -> proc_macro2::TokenStream {
    if let syn::Data::Enum(data) = derive_input.data {
        let packets = data_to_packet_vec(data);
        let name = &derive_input.ident;
        let (id_stream, reliable_stream, ordered_stream) =
            token_streams(name, &packets);
        let gen = quote! {
            impl rudp::PacketDesc for #name {
                fn id(&self) -> u32 {
                    match self {
                        #id_stream
                     }
                }

                fn serialize(&self, writer: &mut Vec<u8>) {
                    use serde_cbor::to_writer;
                    to_writer(writer, self).unwrap();
                }

                fn reliable(&self) -> bool {
                    match self {
                        #reliable_stream
                    }
                }

                fn ordered(id: u32) -> bool {
                    match id {
                        #ordered_stream
                    }
                }

                fn deserialize(_: u32, data: &[u8]) -> Result<Self, rudp::DeserializeError> {
                    use serde_cbor::from_slice;
                    use rudp::DeserializeError;
                    from_slice(data).map_err(|err| {
                        rudp::DeserializeError(format!("serde_cbor error: {:?}", err))
                    })
                }
            }
        };
        gen
    } else {
        panic!("This derive macro is designed only for enum type!");
    }
}

fn data_to_packet_vec(data: DataEnum) -> Vec<Packet> {
    let mut packets = Vec::with_capacity(data.variants.len());
    for var in data.variants.iter() {
        let attr =
            var.attrs.iter().find(|attr| {
                attr.path.is_ident("packet")
            });
        let mut ordered = false;
        let mut reliable = false;
        if let Some(attr) = attr {
            let meta = attr.parse_meta().unwrap();
            if let Meta::List(list) = meta {
                for nested in list.nested {
                    if let NestedMeta::Meta(Meta::Path(path)) = nested {
                        if path.is_ident("ordered") {
                            ordered = true;
                        } else if path.is_ident("reliable") {
                            reliable = true;
                        } else if path.is_ident("unreliable") {
                            reliable = false;
                        } else if path.is_ident("unordered") {
                            ordered = false;
                        }
                    }
                }
            }
        }
        let field_type = match var.fields {
            Fields::Unit => FieldType::Flat,
            Fields::Named(_) => FieldType::Struct,
            Fields::Unnamed(_) => FieldType::Tuple,
        };
        packets.push(Packet {
            reliable,
            ordered,
            name: var.ident.clone(),
            field: field_type,
        });
    }
    packets
}

/// ## Return
/// (id, reliable, ordered)
fn token_streams(ident: &Ident, packets: &Vec<Packet>) -> (proc_macro2::TokenStream, proc_macro2::TokenStream, proc_macro2::TokenStream) {
    let mut id_list: Vec<proc_macro2::TokenStream> = Vec::with_capacity(packets.len());
    let mut reliable_list: Vec<proc_macro2::TokenStream> = Vec::with_capacity(packets.len());
    let mut ordered_list: Vec<proc_macro2::TokenStream> = Vec::with_capacity(packets.len());
    for (id, packet) in packets.iter().enumerate() {
        let id = id as u32;
        let name = &packet.name;
        let reliable = packet.reliable;
        let ordered = packet.ordered;
        let match_id = match packet.field {
            FieldType::Flat => {
                quote! {
                    #name => #id,
                }
            },
            FieldType::Struct => {
                quote! {
                    #name{..} => #id,
                }
            },
            FieldType::Tuple => {
                quote! {
                    #name(_) => #id,
                }
            },
        };
        let match_reliable = match packet.field {
            FieldType::Flat => {
                quote! {
                    #name => #reliable,
                }
            },
            FieldType::Struct => {
                quote! {
                    #name{..} => #reliable,
                }
            },
            FieldType::Tuple => {
                quote! {
                    #name(_) => #reliable,
                }
            },
        };
        let match_ordered = quote! {
            #id => #ordered,
        };
        id_list.push(match_id);
        reliable_list.push(match_reliable);
        ordered_list.push(match_ordered);
    }
    let placeholder = quote! {
        _ => panic!("Invalid ID!"),
    };
    ordered_list.push(placeholder);
    let id_gen = quote! {
        #(#ident::#id_list)*
    };
    let reliable_gen = quote! {
        #(#ident::#reliable_list)*
    };
    let ordered_gen = quote! {
        #(#ordered_list)*
    };
    (id_gen, reliable_gen, ordered_gen)
}
