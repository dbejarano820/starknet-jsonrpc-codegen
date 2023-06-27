use std::{collections::HashSet, str::FromStr};

use anyhow::Result;
use clap::{Parser, ValueEnum};
use regex::Regex;

use crate::spec::*;

mod spec;

mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

const MAX_LINE_LENGTH: usize = 100;

#[derive(Debug, Parser)]
#[clap(author, version, about)]
struct Cli {
    #[clap(long, env, help = "Version of the specification")]
    spec: SpecVersion,
}

#[derive(Debug, Clone)]
struct GenerationProfile {
    version: SpecVersion,
    raw_specs: RawSpecs,
    flatten_options: FlattenOption,
    ignore_types: Vec<String>,
    fixed_field_types: FixedFieldsOptions,
    arc_wrapped_types: ArcWrappingOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpecVersion {
    V0_1_0,
    V0_2_1,
    V0_3_0,
}

#[derive(Debug, Clone)]
struct RawSpecs {
    main: &'static str,
    write: &'static str,
}

#[derive(Debug, Clone)]
struct FixedFieldsOptions {
    fixed_field_types: Vec<RustTypeWithFixedFields>,
}

#[derive(Debug, Clone)]
struct ArcWrappingOptions {
    arc_wrapped_types: Vec<RustTypeWithArcWrappedFields>,
}

#[derive(Debug, Clone)]
struct RustTypeWithFixedFields {
    name: &'static str,
    fields: Vec<FixedField>,
}

#[derive(Debug, Clone)]
struct RustTypeWithArcWrappedFields {
    name: &'static str,
    fields: Vec<&'static str>,
}

#[derive(Debug, Clone)]
struct FixedField {
    name: &'static str,
    value: &'static str,
}

#[derive(Debug, Clone)]
struct TypeResolutionResult {
    model_types: Vec<RustType>,
    request_response_types: Vec<RustType>,
    not_implemented: Vec<String>,
}

#[derive(Debug, Clone)]
struct RustType {
    title: Option<String>,
    description: Option<String>,
    name: String,
    content: RustTypeKind,
}

#[allow(unused)]
#[derive(Debug, Clone)]
enum RustTypeKind {
    Struct(RustStruct),
    Enum(RustEnum),
    Wrapper(RustWrapper),
    Unit(RustUnit),
}

#[derive(Debug, Clone)]
struct RustStruct {
    serde_as_array: bool,
    extra_ref_type: bool,
    fields: Vec<RustField>,
}

#[derive(Debug, Clone)]
struct RustEnum {
    thiserror: bool,
    variants: Vec<RustVariant>,
}

#[derive(Debug, Clone)]
struct RustWrapper {
    type_name: String,
}

#[derive(Debug, Clone)]
struct RustUnit {
    serde_as_array: bool,
}

#[derive(Debug, Clone)]
struct RustField {
    description: Option<String>,
    name: String,
    optional: bool,
    fixed: Option<FixedField>,
    arc_wrap: bool,
    type_name: String,
    serde_rename: Option<String>,
    serde_faltten: bool,
    serializer: Option<SerializerOverride>,
}

#[derive(Debug, Clone)]
struct RustVariant {
    description: Option<String>,
    name: String,
    serde_name: Option<String>,
    error_text: Option<String>,
}

#[derive(Debug, Clone)]
struct RustFieldType {
    type_name: String,
    serializer: Option<SerializerOverride>,
}

#[derive(Debug, Clone)]
enum SerializerOverride {
    Serde(String),
    SerdeAs(String),
}

#[allow(unused)]
#[derive(Debug, Clone)]
enum FlattenOption {
    All,
    Selected(Vec<String>),
}

impl FromStr for SpecVersion {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "0.1.0" | "v0.1.0" => Self::V0_1_0,
            "0.2.1" | "v0.2.1" => Self::V0_2_1,
            "0.3.0" | "v0.3.0" => Self::V0_3_0,
            _ => anyhow::bail!("unknown spec version: {}", s),
        })
    }
}

impl ValueEnum for SpecVersion {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::V0_1_0, Self::V0_2_1, Self::V0_3_0]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        use clap::builder::PossibleValue;

        match self {
            Self::V0_1_0 => Some(PossibleValue::new("0.1.0")),
            Self::V0_2_1 => Some(PossibleValue::new("0.2.1")),
            Self::V0_3_0 => Some(PossibleValue::new("0.3.0")),
        }
    }
}

impl FixedFieldsOptions {
    fn find_fixed_field(&self, type_name: &str, field_name: &str) -> Option<FixedField> {
        self.fixed_field_types.iter().find_map(|item| {
            if item.name == type_name {
                item.fields
                    .iter()
                    .find(|field| field.name == field_name)
                    .cloned()
            } else {
                None
            }
        })
    }
}

impl ArcWrappingOptions {
    fn in_field_wrapped(&self, type_name: &str, field_name: &str) -> bool {
        self.arc_wrapped_types.iter().any(|item| {
            if item.name == type_name {
                item.fields.iter().any(|field| field == &field_name)
            } else {
                false
            }
        })
    }
}

impl RustType {
    pub fn render_stdout(&self) {
        match (self.title.as_ref(), self.description.as_ref()) {
            (Some(title), Some(description)) => {
                print_doc(title, 0);
                println!("///");
                print_doc(description, 0);
            }
            (Some(title), None) => {
                print_doc(title, 0);
            }
            (None, Some(description)) => {
                print_doc(description, 0);
            }
            (None, None) => {}
        }

        self.content.render_stdout(&self.name);
    }

    pub fn render_serde_stdout(&self) {
        match &self.content {
            RustTypeKind::Struct(content) => content.render_serde_stdout(&self.name),
            RustTypeKind::Unit(content) => content.render_serde_stdout(&self.name),
            _ => todo!("serde blocks only implemented for structs and unit"),
        }
    }

    pub fn need_custom_serde(&self) -> bool {
        match &self.content {
            RustTypeKind::Struct(content) => content.need_custom_serde(),
            RustTypeKind::Enum(content) => content.need_custom_serde(),
            RustTypeKind::Wrapper(content) => content.need_custom_serde(),
            RustTypeKind::Unit(content) => content.need_custom_serde(),
        }
    }
}

impl RustTypeKind {
    pub fn render_stdout(&self, name: &str) {
        match self {
            Self::Struct(value) => value.render_stdout(name),
            Self::Enum(value) => value.render_stdout(name),
            Self::Wrapper(value) => value.render_stdout(name),
            Self::Unit(value) => value.render_stdout(name),
        }
    }
}

impl RustStruct {
    pub fn render_stdout(&self, name: &str) {
        let derive_serde = !self.need_custom_serde();

        if derive_serde
            && self
                .fields
                .iter()
                .any(|item| matches!(item.serializer, Some(SerializerOverride::SerdeAs(_))))
        {
            println!("#[serde_as]");
        }
        if derive_serde {
            println!("#[derive(Debug, Clone, Serialize, Deserialize)]");
            println!("#[cfg_attr(feature = \"no_unknown_fields\", serde(deny_unknown_fields))]");
        } else {
            println!("#[derive(Debug, Clone)]");
        }
        println!("pub struct {name} {{");

        for field in self.fields.iter().filter(|field| field.fixed.is_none()) {
            if let Some(doc) = &field.description {
                print_doc(doc, 4);
            }

            for line in field.def_lines(4, derive_serde, false, false) {
                println!("{line}")
            }
        }

        println!("}}");

        if self.extra_ref_type {
            println!();

            print_doc(&format!("Reference version of [{}].", name), 0);
            println!("#[derive(Debug, Clone)]");
            println!("pub struct {name}Ref<'a> {{");

            for field in self.fields.iter().filter(|field| field.fixed.is_none()) {
                for line in field.def_lines(4, false, true, false) {
                    println!("{line}")
                }
            }

            println!("}}");
        }
    }

    pub fn render_serde_stdout(&self, name: &str) {
        self.render_impl_serialize_stdout(name);
        println!();
        self.render_impl_deserialize_stdout(name);
    }

    pub fn need_custom_serde(&self) -> bool {
        self.serde_as_array || self.fields.iter().any(|field| field.fixed.is_some())
    }

    fn render_impl_serialize_stdout(&self, name: &str) {
        if self.serde_as_array {
            self.render_impl_array_serialize_stdout(name);
        } else {
            self.render_impl_tagged_serialize_stdout(name);
        }
    }

    fn render_impl_deserialize_stdout(&self, name: &str) {
        if self.serde_as_array {
            self.render_impl_array_deserialize_stdout(name);
        } else {
            self.render_impl_tagged_deserialize_stdout(name);
        }
    }

    fn render_impl_array_serialize_stdout(&self, name: &str) {
        self.render_impl_array_serialize_stdout_inner(name, false);

        if self.extra_ref_type {
            println!();
            self.render_impl_array_serialize_stdout_inner(name, true);
        }
    }

    fn render_impl_array_serialize_stdout_inner(&self, name: &str, is_ref_type: bool) {
        println!(
            "impl{} Serialize for {}{} {{",
            if is_ref_type { "<'a>" } else { "" },
            name,
            if is_ref_type { "Ref<'a>" } else { "" },
        );
        println!(
            "    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {{"
        );

        for (ind_field, field) in self.fields.iter().enumerate() {
            if field.serializer.is_some() {
                println!("        #[serde_as]");
            }

            println!("        #[derive(Serialize)]");
            println!("        #[serde(transparent)]");
            println!("        struct Field{}<'a> {{", ind_field);
            for line in field.def_lines(12, true, true, false).iter() {
                println!("{line}");
            }
            println!("        }}");
            println!();
        }

        println!("        use serde::ser::SerializeSeq;");
        println!();
        println!("        let mut seq = serializer.serialize_seq(None)?;");
        println!();

        for (ind_field, field) in self.fields.iter().enumerate() {
            if field.name.len() > 5 {
                println!("        seq.serialize_element(&Field{} {{", ind_field);
                println!(
                    "            {}: {}self.{},",
                    field.name,
                    if is_ref_type { "" } else { "&" },
                    field.name
                );
                println!("        }})?;");
            } else {
                println!(
                    "        seq.serialize_element(&Field{} {{ {}: {}self.{} }})?;",
                    ind_field,
                    field.name,
                    if is_ref_type { "" } else { "&" },
                    field.name
                );
            }
        }

        println!();
        println!("        seq.end()");

        println!("    }}");
        println!("}}");
    }

    fn render_impl_tagged_serialize_stdout(&self, name: &str) {
        println!("impl Serialize for {name} {{");
        println!(
            "    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {{"
        );

        if self
            .fields
            .iter()
            .any(|item| matches!(item.serializer, Some(SerializerOverride::SerdeAs(_))))
        {
            println!("        #[serde_as]");
        }

        println!("        #[derive(Serialize)]");
        println!("        struct Tagged<'a> {{");

        for field in self.fields.iter() {
            for line in field.def_lines(12, true, true, false).iter() {
                println!("{line}");
            }
        }

        println!("        }}");
        println!();
        println!("        let tagged = Tagged {{");

        for field in self.fields.iter() {
            match &field.fixed {
                Some(fixed_field) => {
                    println!(
                        "            {}: {},",
                        escape_name(&field.name),
                        fixed_field.value
                    )
                }
                None => println!(
                    "            {}: &self.{},",
                    escape_name(&field.name),
                    escape_name(&field.name)
                ),
            }
        }

        println!("        }};");
        println!();
        println!("        Tagged::serialize(&tagged, serializer)");

        println!("    }}");
        println!("}}");
    }

    fn render_impl_array_deserialize_stdout(&self, name: &str) {
        println!("impl<'de> Deserialize<'de> for {name} {{");
        println!("    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {{");

        println!("        #[serde_as]");
        println!("        #[derive(Deserialize)]");
        println!("        struct AsObject {{");

        for field in self.fields.iter() {
            for line in field.def_lines(12, true, false, false).iter() {
                println!("{line}");
            }
        }

        println!("        }}");
        println!();

        for (ind_field, field) in self.fields.iter().enumerate() {
            if field.serializer.is_some() {
                println!("        #[serde_as]");
            }

            println!("        #[derive(Deserialize)]");
            println!("        #[serde(transparent)]");
            println!("        struct Field{} {{", ind_field);
            for line in field.def_lines(12, true, false, false).iter() {
                println!("{line}");
            }
            println!("        }}");
            println!();
        }

        println!("        let temp = serde_json::Value::deserialize(deserializer)?;");
        println!();
        println!(
            "        if let Ok(mut elements) = Vec::<serde_json::Value>::deserialize(&temp) {{"
        );

        for (ind_field, _) in self.fields.iter().enumerate().rev() {
            println!(
                "            let field{} = serde_json::from_value::<Field{}>(",
                ind_field, ind_field
            );
            println!("                elements");
            println!("                    .pop()");
            println!("                    .ok_or_else(|| serde::de::Error::custom(\"invalid sequence length\"))?,");
            println!("            )");
            println!("            .map_err(|err| serde::de::Error::custom(format!(\"failed to parse element: {{}}\", err)))?;");
        }

        println!();

        println!("            Ok(Self {{");

        for (ind_field, field) in self.fields.iter().enumerate() {
            println!(
                "                {}: field{}.{},",
                field.name, ind_field, field.name
            );
        }

        println!("            }})");

        println!("        }} else if let Ok(object) = AsObject::deserialize(&temp) {{");

        println!("            Ok(Self {{");

        for field in self.fields.iter() {
            println!("                {}: object.{},", field.name, field.name);
        }

        println!("            }})");

        println!("        }} else {{");
        println!("            Err(serde::de::Error::custom(\"invalid sequence length\"))");
        println!("        }}");

        println!("    }}");
        println!("}}");
    }

    fn render_impl_tagged_deserialize_stdout(&self, name: &str) {
        println!("impl<'de> Deserialize<'de> for {name} {{");
        println!("    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {{");

        if self
            .fields
            .iter()
            .any(|item| matches!(item.serializer, Some(SerializerOverride::SerdeAs(_))))
        {
            println!("        #[serde_as]");
        }

        println!("        #[derive(Deserialize)]");
        println!(
            "        #[cfg_attr(feature = \"no_unknown_fields\", serde(deny_unknown_fields))]"
        );
        println!("        struct Tagged {{");

        for field in self.fields.iter() {
            let lines = match &field.fixed {
                Some(_) => RustField {
                    description: field.description.clone(),
                    name: field.name.clone(),
                    optional: true,
                    fixed: None,
                    arc_wrap: false,
                    type_name: format!("Option<{}>", field.type_name),
                    serde_rename: field.serde_rename.clone(),
                    serde_faltten: field.serde_faltten,
                    serializer: field.serializer.as_ref().map(|value| value.to_optional()),
                }
                .def_lines(12, true, false, true),
                None => field.def_lines(12, true, false, true),
            };

            for line in lines.iter() {
                println!("{line}");
            }
        }

        println!("        }}");
        println!();
        println!("        let tagged = Tagged::deserialize(deserializer)?;");
        println!();

        for fixed_field in self.fields.iter().filter_map(|field| field.fixed.as_ref()) {
            println!(
                "        if let Some(tag_field) = &tagged.{} {{",
                escape_name(fixed_field.name)
            );
            println!("            if tag_field != {} {{", fixed_field.value);
            println!(
                "                return Err(serde::de::Error::custom(\"invalid `{}` value\"));",
                fixed_field.name
            );
            println!("            }}");
            println!("        }}");
            println!();
        }

        println!("        Ok(Self {{");

        for field in self.fields.iter().filter(|field| field.fixed.is_none()) {
            println!(
                "            {}: {},",
                escape_name(&field.name),
                if field.arc_wrap {
                    format!("Arc::new(tagged.{})", escape_name(&field.name))
                } else {
                    format!("tagged.{}", escape_name(&field.name))
                }
            );
        }

        println!("        }})");

        println!("    }}");
        println!("}}");
    }
}

impl RustEnum {
    pub fn render_stdout(&self, name: &str) {
        println!(
            "#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize{})]",
            if self.thiserror {
                ", Error"
            } else {
                ""
            }
        );
        println!("pub enum {name} {{");

        for variant in self.variants.iter() {
            if let Some(doc) = &variant.description {
                print_doc(doc, 4);
            }

            if let Some(rename) = &variant.serde_name {
                println!("    #[serde(rename = \"{rename}\")]");
            }
            if let Some(err) = &variant.error_text {
                println!("    #[error(\"{err}\")]");
            }
            println!("    {},", variant.name);
        }

        println!("}}");
    }

    pub fn need_custom_serde(&self) -> bool {
        false
    }
}

impl RustWrapper {
    pub fn render_stdout(&self, name: &str) {
        println!("#[derive(Debug, Clone, Serialize, Deserialize)]");
        println!("pub struct {}(pub {});", name, self.type_name);
    }

    pub fn need_custom_serde(&self) -> bool {
        false
    }
}

impl RustUnit {
    pub fn render_stdout(&self, name: &str) {
        if self.need_custom_serde() {
            println!("#[derive(Debug, Clone)]");
        } else {
            println!("#[derive(Debug, Clone, Serialize, Deserialize)]");
        }
        println!("pub struct {};", name);
    }

    pub fn render_serde_stdout(&self, name: &str) {
        self.render_impl_serialize_stdout(name);
        println!();
        self.render_impl_deserialize_stdout(name);
    }

    pub fn need_custom_serde(&self) -> bool {
        self.serde_as_array
    }

    fn render_impl_serialize_stdout(&self, name: &str) {
        println!("impl Serialize for {name} {{");
        println!(
            "    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {{"
        );

        println!("        use serde::ser::SerializeSeq;");
        println!();
        println!("        let seq = serializer.serialize_seq(Some(0))?;");
        println!("        seq.end()");

        println!("    }}");
        println!("}}");
    }

    fn render_impl_deserialize_stdout(&self, name: &str) {
        println!("impl<'de> Deserialize<'de> for {name} {{");
        println!("    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {{");

        println!("        let elements = Vec::<()>::deserialize(deserializer)?;");
        println!("        if !elements.is_empty() {{");
        println!("            return Err(serde::de::Error::custom(\"invalid sequence length\"));");
        println!("        }}");
        println!("        Ok(Self)");

        println!("    }}");
        println!("}}");
    }
}

impl RustField {
    pub fn def_lines(
        &self,
        leading_spaces: usize,
        serde_attrs: bool,
        is_ref: bool,
        no_arc_wrapping: bool,
    ) -> Vec<String> {
        let mut lines = vec![];

        let leading_spaces = " ".repeat(leading_spaces);

        if serde_attrs {
            if self.optional {
                lines.push(format!(
                    "{leading_spaces}#[serde(skip_serializing_if = \"Option::is_none\")]"
                ));
            }
            if let Some(serde_rename) = &self.serde_rename {
                lines.push(format!(
                    "{leading_spaces}#[serde(rename = \"{serde_rename}\")]"
                ));
            }
            if self.serde_faltten {
                lines.push(format!("{leading_spaces}#[serde(flatten)]"));
            }
            if let Some(serde_as) = &self.serializer {
                lines.push(match serde_as {
                    SerializerOverride::Serde(serializer) => {
                        format!("{leading_spaces}#[serde(with = \"{serializer}\")]")
                    }
                    SerializerOverride::SerdeAs(serializer) => {
                        let serializer = if is_ref && serializer.starts_with("Vec<") {
                            format!("[{}]", &serializer[4..(serializer.len() - 1)])
                        } else {
                            serializer.to_owned()
                        };
                        format!("{leading_spaces}#[serde_as(as = \"{serializer}\")]")
                    }
                });
            }
        }

        lines.push(format!(
            "{}pub {}: {},",
            leading_spaces,
            escape_name(&self.name),
            if is_ref {
                if self.type_name == "String" {
                    String::from("&'a str")
                } else if self.type_name.starts_with("Vec<") {
                    format!("&'a [{}]", &self.type_name[4..(self.type_name.len() - 1)])
                } else {
                    format!("&'a {}", self.type_name)
                }
            } else if self.arc_wrap && !no_arc_wrapping {
                format!("Arc<{}>", self.type_name)
            } else {
                self.type_name.clone()
            },
        ));

        lines
    }
}

impl SerializerOverride {
    pub fn to_optional(&self) -> Self {
        match self {
            SerializerOverride::Serde(_) => {
                todo!("Optional transformation of #[serde(with)] not implemented")
            }
            SerializerOverride::SerdeAs(serde_as) => Self::SerdeAs(format!("Option<{serde_as}>")),
        }
    }
}

fn main() {
    let cli = Cli::parse();

    let profiles: [GenerationProfile; 3] = [
        GenerationProfile {
            version: SpecVersion::V0_1_0,
            raw_specs: RawSpecs {
                main: include_str!("./specs/0.1.0/starknet_api_openrpc.json"),
                write: include_str!("./specs/0.1.0/starknet_write_api.json"),
            },
            flatten_options: FlattenOption::Selected(vec![
                String::from("BLOCK_BODY_WITH_TXS"),
                String::from("BLOCK_BODY_WITH_TX_HASHES"),
            ]),
            ignore_types: vec![],
            fixed_field_types: FixedFieldsOptions {
                fixed_field_types: vec![],
            },
            arc_wrapped_types: ArcWrappingOptions {
                arc_wrapped_types: vec![],
            },
        },
        GenerationProfile {
            version: SpecVersion::V0_2_1,
            raw_specs: RawSpecs {
                main: include_str!("./specs/0.2.1/starknet_api_openrpc.json"),
                write: include_str!("./specs/0.2.1/starknet_write_api.json"),
            },
            flatten_options: FlattenOption::Selected(vec![
                String::from("FUNCTION_CALL"),
                String::from("EVENT"),
                String::from("TYPED_PARAMETER"),
                String::from("BLOCK_BODY_WITH_TXS"),
                String::from("BLOCK_BODY_WITH_TX_HASHES"),
                String::from("BLOCK_HEADER"),
                String::from("BROADCASTED_TXN_COMMON_PROPERTIES"),
                String::from("DEPLOY_ACCOUNT_TXN_PROPERTIES"),
                String::from("DEPLOY_TXN_PROPERTIES"),
                String::from("EVENT_CONTENT"),
                String::from("PENDING_COMMON_RECEIPT_PROPERTIES"),
                String::from("COMMON_TXN_PROPERTIES"),
                String::from("COMMON_RECEIPT_PROPERTIES"),
            ]),
            ignore_types: vec![],
            // We need these because they're implied by the network but not explicitly specified.
            // So it's impossible to dynamically derive them accurately.
            fixed_field_types: FixedFieldsOptions {
                fixed_field_types: vec![
                    RustTypeWithFixedFields {
                        name: "DeclareTransactionV1",
                        fields: vec![
                            FixedField {
                                name: "type",
                                value: "\"DECLARE\"",
                            },
                            FixedField {
                                name: "version",
                                value: "&1",
                            },
                        ],
                    },
                    RustTypeWithFixedFields {
                        name: "DeclareTransactionV2",
                        fields: vec![
                            FixedField {
                                name: "type",
                                value: "\"DECLARE\"",
                            },
                            FixedField {
                                name: "version",
                                value: "&2",
                            },
                        ],
                    },
                    RustTypeWithFixedFields {
                        name: "BroadcastedDeclareTransactionV1",
                        fields: vec![
                            FixedField {
                                name: "type",
                                value: "\"DECLARE\"",
                            },
                            FixedField {
                                name: "version",
                                value: "&1",
                            },
                        ],
                    },
                    RustTypeWithFixedFields {
                        name: "BroadcastedDeclareTransactionV2",
                        fields: vec![
                            FixedField {
                                name: "type",
                                value: "\"DECLARE\"",
                            },
                            FixedField {
                                name: "version",
                                value: "&2",
                            },
                        ],
                    },
                    RustTypeWithFixedFields {
                        name: "DeployAccountTransaction",
                        fields: vec![
                            FixedField {
                                name: "type",
                                value: "\"DEPLOY_ACCOUNT\"",
                            },
                            FixedField {
                                name: "version",
                                value: "&1",
                            },
                        ],
                    },
                    RustTypeWithFixedFields {
                        name: "BroadcastedDeployAccountTransaction",
                        fields: vec![
                            FixedField {
                                name: "type",
                                value: "\"DEPLOY_ACCOUNT\"",
                            },
                            FixedField {
                                name: "version",
                                value: "&1",
                            },
                        ],
                    },
                    RustTypeWithFixedFields {
                        name: "DeployTransaction",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"DEPLOY\"",
                        }],
                    },
                    RustTypeWithFixedFields {
                        name: "BroadcastedDeployTransaction",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"DEPLOY\"",
                        }],
                    },
                    RustTypeWithFixedFields {
                        name: "InvokeTransactionV0",
                        fields: vec![
                            FixedField {
                                name: "type",
                                value: "\"INVOKE\"",
                            },
                            FixedField {
                                name: "version",
                                value: "&0",
                            },
                        ],
                    },
                    RustTypeWithFixedFields {
                        name: "InvokeTransactionV1",
                        fields: vec![
                            FixedField {
                                name: "type",
                                value: "\"INVOKE\"",
                            },
                            FixedField {
                                name: "version",
                                value: "&1",
                            },
                        ],
                    },
                    RustTypeWithFixedFields {
                        name: "BroadcastedInvokeTransactionV0",
                        fields: vec![
                            FixedField {
                                name: "type",
                                value: "\"INVOKE\"",
                            },
                            FixedField {
                                name: "version",
                                value: "&0",
                            },
                        ],
                    },
                    RustTypeWithFixedFields {
                        name: "BroadcastedInvokeTransactionV1",
                        fields: vec![
                            FixedField {
                                name: "type",
                                value: "\"INVOKE\"",
                            },
                            FixedField {
                                name: "version",
                                value: "&1",
                            },
                        ],
                    },
                    RustTypeWithFixedFields {
                        name: "L1HandlerTransaction",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"L1_HANDLER\"",
                        }],
                    },
                    RustTypeWithFixedFields {
                        name: "InvokeTransactionReceipt",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"INVOKE\"",
                        }],
                    },
                    RustTypeWithFixedFields {
                        name: "DeclareTransactionReceipt",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"DECLARE\"",
                        }],
                    },
                    RustTypeWithFixedFields {
                        name: "DeployAccountTransactionReceipt",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"DEPLOY_ACCOUNT\"",
                        }],
                    },
                    RustTypeWithFixedFields {
                        name: "DeployTransactionReceipt",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"DEPLOY\"",
                        }],
                    },
                    RustTypeWithFixedFields {
                        name: "L1HandlerTransactionReceipt",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"L1_HANDLER\"",
                        }],
                    },
                    RustTypeWithFixedFields {
                        name: "PendingInvokeTransactionReceipt",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"INVOKE\"",
                        }],
                    },
                    RustTypeWithFixedFields {
                        name: "PendingDeclareTransactionReceipt",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"DECLARE\"",
                        }],
                    },
                    RustTypeWithFixedFields {
                        name: "PendingDeployAccountTransactionReceipt",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"DEPLOY_ACCOUNT\"",
                        }],
                    },
                    RustTypeWithFixedFields {
                        name: "PendingDeployTransactionReceipt",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"DEPLOY\"",
                        }],
                    },
                    RustTypeWithFixedFields {
                        name: "PendingL1HandlerTransactionReceipt",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"L1_HANDLER\"",
                        }],
                    },
                ],
            },
            arc_wrapped_types: ArcWrappingOptions {
                arc_wrapped_types: vec![
                    RustTypeWithArcWrappedFields {
                        name: "BroadcastedDeclareTransactionV1",
                        fields: vec!["contract_class"],
                    },
                    RustTypeWithArcWrappedFields {
                        name: "BroadcastedDeclareTransactionV2",
                        fields: vec!["contract_class"],
                    },
                ],
            },
        },
        GenerationProfile {
            version: SpecVersion::V0_3_0,
            raw_specs: RawSpecs {
                main: include_str!("./specs/0.3.0/starknet_api_openrpc.json"),
                write: include_str!("./specs/0.3.0/starknet_write_api.json"),
            },
            flatten_options: FlattenOption::Selected(vec![
                String::from("FUNCTION_CALL"),
                String::from("EVENT"),
                String::from("TYPED_PARAMETER"),
                String::from("BLOCK_BODY_WITH_TXS"),
                String::from("BLOCK_BODY_WITH_TX_HASHES"),
                String::from("BLOCK_HEADER"),
                String::from("BROADCASTED_TXN_COMMON_PROPERTIES"),
                String::from("DEPLOY_ACCOUNT_TXN_PROPERTIES"),
                String::from("DEPLOY_TXN_PROPERTIES"),
                String::from("EVENT_CONTENT"),
                String::from("PENDING_COMMON_RECEIPT_PROPERTIES"),
                String::from("COMMON_TXN_PROPERTIES"),
                String::from("COMMON_RECEIPT_PROPERTIES"),
                String::from("PENDING_STATE_UPDATE"),
                String::from("DECLARE_TXN_V1"),
            ]),
            ignore_types: vec![],
            // We need these because they're implied by the network but not explicitly specified.
            // So it's impossible to dynamically derive them accurately.
            fixed_field_types: FixedFieldsOptions {
                fixed_field_types: vec![
                    RustTypeWithFixedFields {
                        name: "DeclareTransactionV1",
                        fields: vec![
                            FixedField {
                                name: "type",
                                value: "\"DECLARE\"",
                            },
                            FixedField {
                                name: "version",
                                value: "&1",
                            },
                        ],
                    },
                    RustTypeWithFixedFields {
                        name: "DeclareTransactionV2",
                        fields: vec![
                            FixedField {
                                name: "type",
                                value: "\"DECLARE\"",
                            },
                            FixedField {
                                name: "version",
                                value: "&2",
                            },
                        ],
                    },
                    RustTypeWithFixedFields {
                        name: "BroadcastedDeclareTransactionV1",
                        fields: vec![
                            FixedField {
                                name: "type",
                                value: "\"DECLARE\"",
                            },
                            FixedField {
                                name: "version",
                                value: "&1",
                            },
                        ],
                    },
                    RustTypeWithFixedFields {
                        name: "BroadcastedDeclareTransactionV2",
                        fields: vec![
                            FixedField {
                                name: "type",
                                value: "\"DECLARE\"",
                            },
                            FixedField {
                                name: "version",
                                value: "&2",
                            },
                        ],
                    },
                    RustTypeWithFixedFields {
                        name: "DeployAccountTransaction",
                        fields: vec![
                            FixedField {
                                name: "type",
                                value: "\"DEPLOY_ACCOUNT\"",
                            },
                            FixedField {
                                name: "version",
                                value: "&1",
                            },
                        ],
                    },
                    RustTypeWithFixedFields {
                        name: "BroadcastedDeployAccountTransaction",
                        fields: vec![
                            FixedField {
                                name: "type",
                                value: "\"DEPLOY_ACCOUNT\"",
                            },
                            FixedField {
                                name: "version",
                                value: "&1",
                            },
                        ],
                    },
                    RustTypeWithFixedFields {
                        name: "DeployTransaction",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"DEPLOY\"",
                        }],
                    },
                    RustTypeWithFixedFields {
                        name: "InvokeTransactionV0",
                        fields: vec![
                            FixedField {
                                name: "type",
                                value: "\"INVOKE\"",
                            },
                            FixedField {
                                name: "version",
                                value: "&0",
                            },
                        ],
                    },
                    RustTypeWithFixedFields {
                        name: "InvokeTransactionV1",
                        fields: vec![
                            FixedField {
                                name: "type",
                                value: "\"INVOKE\"",
                            },
                            FixedField {
                                name: "version",
                                value: "&1",
                            },
                        ],
                    },
                    RustTypeWithFixedFields {
                        name: "BroadcastedInvokeTransactionV0",
                        fields: vec![
                            FixedField {
                                name: "type",
                                value: "\"INVOKE\"",
                            },
                            FixedField {
                                name: "version",
                                value: "&0",
                            },
                        ],
                    },
                    RustTypeWithFixedFields {
                        name: "BroadcastedInvokeTransactionV1",
                        fields: vec![
                            FixedField {
                                name: "type",
                                value: "\"INVOKE\"",
                            },
                            FixedField {
                                name: "version",
                                value: "&1",
                            },
                        ],
                    },
                    RustTypeWithFixedFields {
                        name: "L1HandlerTransaction",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"L1_HANDLER\"",
                        }],
                    },
                    RustTypeWithFixedFields {
                        name: "InvokeTransactionReceipt",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"INVOKE\"",
                        }],
                    },
                    RustTypeWithFixedFields {
                        name: "DeclareTransactionReceipt",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"DECLARE\"",
                        }],
                    },
                    RustTypeWithFixedFields {
                        name: "DeployAccountTransactionReceipt",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"DEPLOY_ACCOUNT\"",
                        }],
                    },
                    RustTypeWithFixedFields {
                        name: "DeployTransactionReceipt",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"DEPLOY\"",
                        }],
                    },
                    RustTypeWithFixedFields {
                        name: "L1HandlerTransactionReceipt",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"L1_HANDLER\"",
                        }],
                    },
                    RustTypeWithFixedFields {
                        name: "PendingInvokeTransactionReceipt",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"INVOKE\"",
                        }],
                    },
                    RustTypeWithFixedFields {
                        name: "PendingDeclareTransactionReceipt",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"DECLARE\"",
                        }],
                    },
                    RustTypeWithFixedFields {
                        name: "PendingDeployAccountTransactionReceipt",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"DEPLOY_ACCOUNT\"",
                        }],
                    },
                    RustTypeWithFixedFields {
                        name: "PendingDeployTransactionReceipt",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"DEPLOY\"",
                        }],
                    },
                    RustTypeWithFixedFields {
                        name: "PendingL1HandlerTransactionReceipt",
                        fields: vec![FixedField {
                            name: "type",
                            value: "\"L1_HANDLER\"",
                        }],
                    },
                ],
            },
            arc_wrapped_types: ArcWrappingOptions {
                arc_wrapped_types: vec![
                    RustTypeWithArcWrappedFields {
                        name: "BroadcastedDeclareTransactionV1",
                        fields: vec!["contract_class"],
                    },
                    RustTypeWithArcWrappedFields {
                        name: "BroadcastedDeclareTransactionV2",
                        fields: vec!["contract_class"],
                    },
                ],
            },
        },
    ];

    let profile = profiles
        .into_iter()
        .find(|profile| profile.version == cli.spec)
        .expect("Unable to find profile");

    let mut specs: Specification =
        serde_json::from_str(profile.raw_specs.main).expect("Failed to parse specification");

    // Merge specs (we only care about write methods and errors at the moment as the write specs
    // does not provide additional models).
    let mut write_specs: Specification =
        serde_json::from_str(profile.raw_specs.write).expect("Failed to parse specification");
    specs.methods.append(&mut write_specs.methods);
    write_specs
        .components
        .errors
        .iter()
        .for_each(|(key, value)| {
            if let indexmap::map::Entry::Vacant(entry) =
                specs.components.errors.entry(key.to_owned())
            {
                entry.insert(value.to_owned());
            }
        });

    println!("// AUTO-GENERATED CODE. DO NOT EDIT");
    println!("// To change the code generated, modify the codegen tool instead:");
    println!("//     https://github.com/xJonathanLEI/starknet-jsonrpc-codegen");
    println!();
    println!("// Code generated with version:");
    match built_info::GIT_COMMIT_HASH {
        Some(commit_hash) => println!(
            "//     https://github.com/xJonathanLEI/starknet-jsonrpc-codegen#{commit_hash}"
        ),
        None => println!("    <Unable to determine Git commit hash>"),
    }
    println!();

    if !profile.ignore_types.is_empty() {
        println!("// These types are ignored from code generation. Implement them manually:");
        for ignored_type in profile.ignore_types.iter() {
            println!("// - `{ignored_type}`");
        }
        println!();
    }

    let result = resolve_types(
        &specs,
        &profile.flatten_options,
        &profile.ignore_types,
        &profile.fixed_field_types,
        &profile.arc_wrapped_types,
    )
    .expect("Failed to resolve types");

    if !result.not_implemented.is_empty() {
        println!("// Code generation requested but not implemented for these types:");
        for type_name in result.not_implemented.iter() {
            println!("// - `{type_name}`");
        }
        println!();
    }

    println!("use serde::{{Deserialize, Deserializer, Serialize, Serializer}};");
    println!("use serde_with::serde_as;");

    if profile.version == SpecVersion::V0_1_0 {
        println!("use starknet_core::{{");
        println!("    serde::{{byte_array::base64, unsigned_field_element::UfeHex}},");
        println!("    types::FieldElement,");
        println!("}};");
    } else {
        println!();
        println!("use crate::{{");
        println!("    serde::{{byte_array::base64, unsigned_field_element::UfeHex}},");
        println!("    types::FieldElement,");
        println!("    stdlib::sync::Arc,");
        println!("    stdlib::Error,");
        println!("}};");
    }

    println!();

    // In later versions this type is still defined by never actually used
    if profile.version == SpecVersion::V0_1_0 {
        println!("pub use starknet_core::types::L1Address as EthAddress;");
        println!();
    }

    println!("use super::{{serde_impls::NumAsHex, *}};");
    println!();

    let mut manual_serde_types = vec![];

    for rust_type in result
        .model_types
        .iter()
        .chain(result.request_response_types.iter())
    {
        if rust_type.need_custom_serde() {
            manual_serde_types.push(rust_type);
        }

        rust_type.render_stdout();

        println!();
    }

    for (ind, rust_type) in manual_serde_types.iter().enumerate() {
        rust_type.render_serde_stdout();

        if ind != manual_serde_types.len() - 1 {
            println!();
        }
    }
}

fn resolve_types(
    specs: &Specification,
    flatten_option: &FlattenOption,
    ignore_types: &[String],
    fixed_fields: &FixedFieldsOptions,
    arc_wrapping: &ArcWrappingOptions,
) -> Result<TypeResolutionResult> {
    let mut types = vec![];
    let mut req_types: Vec<RustType> = vec![];
    let mut not_implemented_types = vec![];

    let flatten_only_types = get_flatten_only_schemas(specs, flatten_option);

    for (name, entity) in specs.components.schemas.iter() {
        let rusty_name = to_starknet_rs_name(name);

        let title = entity.title();
        let description = match entity.description() {
            Some(description) => Some(description),
            None => entity.summary(),
        };

        // Explicitly ignored types
        if ignore_types.contains(name) {
            continue;
        }

        // Manual override exists
        if get_field_type_override(name).is_some() {
            continue;
        }

        if flatten_only_types.contains(name) {
            continue;
        }

        let mut content = match schema_to_rust_type_kind(specs, entity, flatten_option)? {
            Some(content) => content,
            None => {
                not_implemented_types.push(name.to_owned());

                eprintln!("OneOf enum generation not implemented. Enum not generated for {name}");
                continue;
            }
        };

        if let RustTypeKind::Struct(inner) = &mut content {
            for field in inner.fields.iter_mut() {
                field.fixed = fixed_fields.find_fixed_field(&rusty_name, &field.name);
                field.arc_wrap = arc_wrapping.in_field_wrapped(&rusty_name, &field.name);
            }
        }

        types.push(RustType {
            title: title.map(|value| to_starknet_rs_doc(value, true)),
            description: description.map(|value| to_starknet_rs_doc(value, true)),
            name: rusty_name,
            content,
        });
    }

    types.push(RustType {
        title: Some(String::from("JSON-RPC error codes")),
        description: None,
        name: String::from("StarknetError"),
        content: RustTypeKind::Enum(RustEnum {
            thiserror: true,
            variants: specs
                .components
                .errors
                .iter()
                .map(|(name, err)| match err {
                    ErrorType::Error(err) => RustVariant {
                        description: Some(err.message.clone()),
                        name: to_starknet_rs_name(name),
                        serde_name: None,
                        error_text: Some(err.message.clone()),
                    },
                    ErrorType::Reference(_) => todo!("Error redirection not implemented"),
                })
                .collect(),
        }),
    });

    // Request/response types
    for method in specs.methods.iter() {
        let mut request_fields = vec![];

        for param in method.params.iter() {
            let field_type = get_rust_type_for_field(&param.schema, specs)?;

            request_fields.push(RustField {
                description: param.description.clone(),
                name: param.name.clone(),
                optional: !param.required,
                fixed: None,
                arc_wrap: false,
                type_name: field_type.type_name,
                serde_rename: None,
                serde_faltten: false,
                serializer: field_type.serializer,
            });
        }

        let request_type = RustType {
            title: Some(format!("Request for method {}", method.name)),
            description: None,
            name: format!(
                "{}Request",
                to_starknet_rs_name(&camel_to_snake_case(
                    method.name.trim_start_matches("starknet_")
                ))
            ),
            content: if request_fields.is_empty() {
                RustTypeKind::Unit(RustUnit {
                    serde_as_array: true,
                })
            } else {
                RustTypeKind::Struct(RustStruct {
                    serde_as_array: true,
                    extra_ref_type: true,
                    fields: request_fields,
                })
            },
        };

        req_types.push(request_type);
    }

    // Sorting the types makes it easier to check diffs in generated code.
    types.sort_by_key(|item| item.name.to_owned());
    req_types.sort_by_key(|item| item.name.to_owned());
    not_implemented_types.sort();

    Ok(TypeResolutionResult {
        model_types: types,
        request_response_types: req_types,
        not_implemented: not_implemented_types,
    })
}

fn schema_to_rust_type_kind(
    specs: &Specification,
    entity: &Schema,
    flatten_option: &FlattenOption,
) -> Result<Option<RustTypeKind>> {
    Ok(match entity {
        Schema::Ref(reference) => {
            let mut fields = vec![];
            let redirected_schema = specs
                .components
                .schemas
                .get(reference.name())
                .ok_or_else(|| anyhow::anyhow!(""))?;
            get_schema_fields(redirected_schema, specs, &mut fields, flatten_option)?;
            Some(RustTypeKind::Struct(RustStruct {
                serde_as_array: false,
                extra_ref_type: false,
                fields,
            }))
        }
        Schema::OneOf(_) => None,
        Schema::AllOf(_) | Schema::Primitive(Primitive::Object(_)) => {
            let mut fields = vec![];
            get_schema_fields(entity, specs, &mut fields, flatten_option)?;
            Some(RustTypeKind::Struct(RustStruct {
                serde_as_array: false,
                extra_ref_type: false,
                fields,
            }))
        }
        Schema::Primitive(Primitive::String(value)) => match &value.r#enum {
            Some(variants) => Some(RustTypeKind::Enum(RustEnum {
                thiserror: false,
                variants: variants
                    .iter()
                    .map(|item| RustVariant {
                        description: None,
                        name: to_starknet_rs_name(item),
                        serde_name: Some(item.to_owned()),
                        error_text: None,
                    })
                    .collect(),
            })),
            None => {
                anyhow::bail!("Unexpected non-enum string type when generating struct/enum");
            }
        },
        _ => {
            anyhow::bail!("Unexpected schema type when generating struct/enum");
        }
    })
}

/// Finds the list of schemas that are used and only used for flattening inside objects
fn get_flatten_only_schemas(specs: &Specification, flatten_option: &FlattenOption) -> Vec<String> {
    // We need this for now since we don't search method calls, so we could get false positives
    const HARD_CODED_NON_FLATTEN_SCHEMAS: [&str; 2] = ["FUNCTION_CALL", "PENDING_STATE_UPDATE"];

    let mut flatten_fields = HashSet::<String>::new();
    let mut non_flatten_fields = HashSet::<String>::new();

    for (_, schema) in specs.components.schemas.iter() {
        visit_schema_for_flatten_only(
            schema,
            flatten_option,
            &mut flatten_fields,
            &mut non_flatten_fields,
        );
    }

    flatten_fields
        .into_iter()
        .filter_map(|item| {
            if non_flatten_fields.contains(&item)
                || HARD_CODED_NON_FLATTEN_SCHEMAS.contains(&item.as_str())
            {
                None
            } else {
                Some(item)
            }
        })
        .collect()
}

fn visit_schema_for_flatten_only(
    schema: &Schema,
    flatten_option: &FlattenOption,
    flatten_fields: &mut HashSet<String>,
    non_flatten_fields: &mut HashSet<String>,
) {
    match schema {
        Schema::OneOf(one_of) => {
            // Recursion
            for variant in one_of.one_of.iter() {
                match variant {
                    Schema::Ref(reference) => {
                        non_flatten_fields.insert(reference.name().to_owned());
                    }
                    _ => visit_schema_for_flatten_only(
                        variant,
                        flatten_option,
                        flatten_fields,
                        non_flatten_fields,
                    ),
                }
            }
        }
        Schema::AllOf(all_of) => {
            for fragment in all_of.all_of.iter() {
                match fragment {
                    Schema::Ref(reference) => {
                        let should_flatten = match flatten_option {
                            FlattenOption::All => true,
                            FlattenOption::Selected(flatten_types) => {
                                flatten_types.contains(&reference.name().to_owned())
                            }
                        };

                        if should_flatten {
                            flatten_fields.insert(reference.name().to_owned());
                        } else {
                            non_flatten_fields.insert(reference.name().to_owned());
                            visit_schema_for_flatten_only(
                                fragment,
                                flatten_option,
                                flatten_fields,
                                non_flatten_fields,
                            );
                        }
                    }
                    _ => visit_schema_for_flatten_only(
                        fragment,
                        flatten_option,
                        flatten_fields,
                        non_flatten_fields,
                    ),
                }
            }
        }
        Schema::Primitive(Primitive::Object(object)) => {
            for (_, prop_type) in object.properties.iter() {
                match prop_type {
                    Schema::Ref(reference) => {
                        non_flatten_fields.insert(reference.name().to_owned());
                    }
                    _ => visit_schema_for_flatten_only(
                        prop_type,
                        flatten_option,
                        flatten_fields,
                        non_flatten_fields,
                    ),
                }
            }
        }
        Schema::Primitive(Primitive::Array(array)) => match array.items.as_ref() {
            Schema::Ref(reference) => {
                non_flatten_fields.insert(reference.name().to_owned());
            }
            _ => visit_schema_for_flatten_only(
                &array.items,
                flatten_option,
                flatten_fields,
                non_flatten_fields,
            ),
        },
        _ => {}
    }
}

fn get_schema_fields(
    schema: &Schema,
    specs: &Specification,
    fields: &mut Vec<RustField>,
    flatten_option: &FlattenOption,
) -> Result<()> {
    match schema {
        Schema::Ref(value) => {
            let ref_type_name = value.name();
            let ref_type = match specs.components.schemas.get(ref_type_name) {
                Some(ref_type) => ref_type,
                None => anyhow::bail!("Ref target type not found: {}", ref_type_name),
            };

            // Schema redirection
            get_schema_fields(ref_type, specs, fields, flatten_option)?;
        }
        Schema::AllOf(value) => {
            for item in value.all_of.iter() {
                match item {
                    Schema::Ref(reference) => {
                        let should_flatten = match flatten_option {
                            FlattenOption::All => true,
                            FlattenOption::Selected(flatten_types) => {
                                flatten_types.contains(&reference.name().to_owned())
                            }
                        };

                        if should_flatten {
                            get_schema_fields(item, specs, fields, flatten_option)?;
                        } else {
                            fields.push(RustField {
                                description: reference.description.to_owned(),
                                name: reference.name().to_lowercase(),
                                optional: false,
                                fixed: None,
                                arc_wrap: false,
                                type_name: to_starknet_rs_name(reference.name()),
                                serde_rename: None,
                                serde_faltten: true,
                                serializer: None,
                            });
                        }
                    }
                    _ => {
                        // We don't have a choice but to flatten it
                        get_schema_fields(item, specs, fields, flatten_option)?;
                    }
                }
            }
        }
        Schema::Primitive(Primitive::Object(value)) => {
            for (name, prop_value) in value.properties.iter() {
                // For fields we keep things simple and only use one line
                let doc_string = match prop_value.description() {
                    Some(text) => Some(text),
                    None => match prop_value.title() {
                        Some(text) => Some(text),
                        None => prop_value.summary(),
                    },
                };

                let field_type = get_rust_type_for_field(prop_value, specs)?;

                let field_name = to_rust_field_name(name);
                let rename = if name == &field_name {
                    None
                } else {
                    Some(name.to_owned())
                };

                // Optional field transformation
                let field_optional = match &value.required {
                    Some(required) => !required.contains(name),
                    None => false,
                };
                let type_name = if field_optional {
                    format!("Option<{}>", field_type.type_name)
                } else {
                    field_type.type_name
                };
                let serializer = if field_optional {
                    field_type.serializer.map(|value| value.to_optional())
                } else {
                    field_type.serializer
                };

                fields.push(RustField {
                    description: doc_string.map(|value| to_starknet_rs_doc(value, false)),
                    name: field_name,
                    optional: field_optional,
                    fixed: None,
                    arc_wrap: false,
                    type_name,
                    serde_rename: rename,
                    serde_faltten: false,
                    serializer,
                });
            }
        }
        _ => {
            dbg!(schema);
            anyhow::bail!("Unexpected schema type when getting object fields");
        }
    }

    Ok(())
}

fn get_rust_type_for_field(schema: &Schema, specs: &Specification) -> Result<RustFieldType> {
    match schema {
        Schema::Ref(value) => {
            let ref_type_name = value.name();
            if !specs.components.schemas.contains_key(ref_type_name) {
                anyhow::bail!("Ref target type not found: {}", ref_type_name);
            }

            // Hard-coded special rules
            Ok(
                get_field_type_override(ref_type_name).unwrap_or_else(|| RustFieldType {
                    type_name: to_starknet_rs_name(ref_type_name),
                    serializer: None,
                }),
            )
        }
        Schema::OneOf(_) => {
            anyhow::bail!("Anonymous oneOf types should not be used for properties");
        }
        Schema::AllOf(_) => {
            anyhow::bail!("Anonymous allOf types should not be used for properties");
        }
        Schema::Primitive(value) => match value {
            Primitive::Array(value) => {
                let item_type = get_rust_type_for_field(&value.items, specs)?;
                let serializer = match item_type.serializer {
                    Some(SerializerOverride::Serde(_)) => {
                        todo!("Array wrapper for #[serde(with)] not implemented")
                    }
                    Some(SerializerOverride::SerdeAs(serializer)) => {
                        Some(SerializerOverride::SerdeAs(format!("Vec<{serializer}>")))
                    }
                    None => None,
                };
                Ok(RustFieldType {
                    type_name: format!("Vec<{}>", item_type.type_name),
                    serializer,
                })
            }
            Primitive::Boolean(_) => Ok(RustFieldType {
                type_name: String::from("bool"),
                serializer: None,
            }),
            Primitive::Integer(_) => Ok(RustFieldType {
                type_name: String::from("u64"),
                serializer: None,
            }),
            Primitive::Object(_) => {
                anyhow::bail!("Anonymous object types should not be used for properties");
            }
            Primitive::String(value) => {
                // Hacky solution but it's the best we can do given the specs
                if let Some(desc) = &value.description {
                    if desc.contains("base64") {
                        return Ok(RustFieldType {
                            type_name: String::from("Vec<u8>"),
                            serializer: Some(SerializerOverride::Serde(String::from("base64"))),
                        });
                    }
                }

                Ok(RustFieldType {
                    type_name: String::from("String"),
                    serializer: None,
                })
            }
        },
    }
}

fn get_field_type_override(type_name: &str) -> Option<RustFieldType> {
    Some(match type_name {
        "ADDRESS" | "STORAGE_KEY" | "TXN_HASH" | "FELT" | "BLOCK_HASH" | "CHAIN_ID"
        | "PROTOCOL_VERSION" => RustFieldType {
            type_name: String::from("FieldElement"),
            serializer: Some(SerializerOverride::SerdeAs(String::from("UfeHex"))),
        },
        "BLOCK_NUMBER" => RustFieldType {
            type_name: String::from("u64"),
            serializer: None,
        },
        "NUM_AS_HEX" => RustFieldType {
            type_name: String::from("u64"),
            serializer: Some(SerializerOverride::SerdeAs(String::from("NumAsHex"))),
        },
        "ETH_ADDRESS" => RustFieldType {
            type_name: String::from("EthAddress"),
            serializer: None,
        },
        "SIGNATURE" => RustFieldType {
            type_name: String::from("Vec<FieldElement>"),
            serializer: Some(SerializerOverride::SerdeAs(String::from("Vec<UfeHex>"))),
        },
        "CONTRACT_ABI" => RustFieldType {
            type_name: String::from("Vec<LegacyContractAbiEntry>"),
            serializer: None,
        },
        "CONTRACT_ENTRY_POINT_LIST" => RustFieldType {
            type_name: String::from("Vec<ContractEntryPoint>"),
            serializer: None,
        },
        "LEGACY_CONTRACT_ENTRY_POINT_LIST" => RustFieldType {
            type_name: String::from("Vec<LegacyContractEntryPoint>"),
            serializer: None,
        },
        "TXN_TYPE" => RustFieldType {
            type_name: String::from("String"),
            serializer: None,
        },
        _ => return None,
    })
}

fn print_doc(doc: &str, indent_spaces: usize) {
    let prefix = format!("{}/// ", " ".repeat(indent_spaces));
    for line in wrap_lines(doc, prefix.len()) {
        println!("{prefix}{line}");
    }
}

fn wrap_lines(doc: &str, prefix_length: usize) -> Vec<String> {
    let mut lines = vec![];
    let mut current_line = String::new();

    for part in doc.split(' ') {
        let mut addition = String::new();
        if !current_line.is_empty() {
            addition.push(' ');
        }
        addition.push_str(part);

        if prefix_length + current_line.len() + addition.len() <= MAX_LINE_LENGTH {
            current_line.push_str(&addition);
        } else {
            lines.push(current_line.clone());
            current_line.clear();
            current_line.push_str(part);
        }
    }

    lines.push(current_line);
    lines
}

fn to_starknet_rs_name(name: &str) -> String {
    let name = to_pascal_case(name).replace("Txn", "Transaction");

    // Hard-coded renames
    match name.as_ref() {
        "CommonTransactionProperties" => String::from("TransactionMeta"),
        "CommonReceiptProperties" => String::from("TransactionReceiptMeta"),
        "InvokeTransactionReceiptProperties" => String::from("InvokeTransactionReceiptData"),
        "PendingCommonReceiptProperties" => String::from("PendingTransactionReceiptMeta"),
        "SierraContractClass" => String::from("FlattenedSierraClass"),
        "LegacyContractClass" => String::from("CompressedLegacyContractClass"),
        "DeprecatedContractClass" => String::from("CompressedLegacyContractClass"),
        "ContractAbiEntry" => String::from("LegacyContractAbiEntry"),
        "FunctionAbiEntry" => String::from("LegacyFunctionAbiEntry"),
        "EventAbiEntry" => String::from("LegacyEventAbiEntry"),
        "StructAbiEntry" => String::from("LegacyStructAbiEntry"),
        "FunctionAbiType" => String::from("LegacyFunctionAbiType"),
        "EventAbiType" => String::from("LegacyEventAbiType"),
        "StructAbiType" => String::from("LegacyStructAbiType"),
        "StructMember" => String::from("LegacyStructMember"),
        "TypedParameter" => String::from("LegacyTypedParameter"),
        "DeprecatedEntryPointsByType" => String::from("LegacyEntryPointsByType"),
        "DeprecatedCairoEntryPoint" => String::from("LegacyContractEntryPoint"),
        _ => name,
    }
}

fn to_rust_field_name(name: &str) -> String {
    let all_upper_letters_regex = Regex::new("^[A-Z]+$").unwrap();

    if all_upper_letters_regex.is_match(name) || name.contains('_') {
        // Already snake case
        name.to_ascii_lowercase()
    } else {
        camel_to_snake_case(name)
    }
}

fn to_starknet_rs_doc(doc: &str, force_period: bool) -> String {
    let mut doc = to_sentence_case(doc);

    for (pattern, target) in [
        (Regex::new(r"(?i)\bethereum\b").unwrap(), "Ethereum"),
        (Regex::new(r"(?i)\bstarknet\b").unwrap(), "Starknet"),
        (Regex::new(r"(?i)\bstarknet\.io\b").unwrap(), "starknet.io"),
        (Regex::new(r"\bl1\b").unwrap(), "L1"),
        (Regex::new(r"\bl2\b").unwrap(), "L2"),
        (Regex::new(r"\bunix\b").unwrap(), "Unix"),
    ]
    .into_iter()
    {
        doc = pattern.replace_all(&doc, target).into_owned();
    }

    if force_period && !doc.ends_with('.') {
        doc.push('.');
    }

    doc
}

fn to_pascal_case(name: &str) -> String {
    let mut result = String::new();

    let mut last_underscore = None;
    for (ind, character) in name.chars().enumerate() {
        if character == '_' {
            last_underscore = Some(ind);
            continue;
        }

        let uppercase = match last_underscore {
            Some(last_underscore) => ind == last_underscore + 1,
            None => ind == 0,
        };

        result.push(if uppercase {
            character.to_ascii_uppercase()
        } else {
            character.to_ascii_lowercase()
        });
    }

    result
}

fn camel_to_snake_case(name: &str) -> String {
    let mut result = String::new();

    for character in name.chars() {
        let is_upper = character.to_ascii_uppercase() == character;
        if is_upper {
            result.push('_');
            result.push(character.to_ascii_lowercase());
        } else {
            result.push(character);
        }
    }

    result
}

fn to_sentence_case(name: &str) -> String {
    let mut result = String::new();

    let mut last_period = None;
    let mut last_char = None;

    for (ind, character) in name.chars().enumerate() {
        if character == '.' {
            last_period = Some(ind);
        }

        let uppercase = match last_period {
            Some(last_period) => ind == last_period + 2 && matches!(last_char, Some(' ')),
            None => ind == 0,
        };

        result.push(if uppercase {
            character.to_ascii_uppercase()
        } else {
            character.to_ascii_lowercase()
        });

        last_char = Some(character);
    }

    result
}

fn escape_name(name: &str) -> &str {
    if name == "type" {
        "r#type"
    } else {
        name
    }
}
