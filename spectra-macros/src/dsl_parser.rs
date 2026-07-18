use syn::parse::{Parse, ParseStream};
use syn::{braced, Ident, LitFloat, LitInt, Token};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpectraLevelSpec {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl SpectraLevelSpec {
    pub fn from_ident(ident: &Ident) -> syn::Result<Self> {
        match ident.to_string().as_str() {
            "Error" => Ok(Self::Error),
            "Warn" => Ok(Self::Warn),
            "Info" => Ok(Self::Info),
            "Debug" => Ok(Self::Debug),
            "Trace" => Ok(Self::Trace),
            other => Err(syn::Error::new(
                ident.span(),
                format!("unknown Spectra level `{other}`; expected Error, Warn, Info, Debug, or Trace"),
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FieldSpec {
    pub name: String,
    pub rust_type: String,
    pub pii: bool,
    pub safe_for_console: bool,
}

#[derive(Debug, Clone)]
pub struct EventSchemaSpec {
    pub schema_name: String,
    pub table: String,
    pub store: Option<String>,
    pub version: String,
    pub description: Option<String>,
    pub level: Option<SpectraLevelSpec>,
    pub default_sample_rate: Option<f64>,
    pub fields: Vec<FieldSpec>,
}

#[derive(Debug, Clone)]
pub struct MetricSchemaSpec {
    pub schema_name: String,
    pub name: String,
    pub store: Option<String>,
    pub version: String,
    pub description: Option<String>,
    pub level: Option<SpectraLevelSpec>,
    pub default_sample_rate: Option<f64>,
    pub coalesce_ms: Option<u64>,
}

impl Parse for EventSchemaSpec {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let schema_name: Ident = input.parse()?;
        let content;
        braced!(content in input);
        let mut table = None;
        let mut store = None;
        let mut version = None;
        let mut description = None;
        let mut level = None;
        let mut default_sample_rate = None;
        let mut coalesce_ms = None;
        let mut fields = Vec::new();

        while !content.is_empty() {
            let key: Ident = content.parse()?;
            content.parse::<Token![:]>()?;
            let key_str = key.to_string();
            match key_str.as_str() {
                "table" => {
                    let lit: syn::LitStr = content.parse()?;
                    table = Some(lit.value());
                }
                "store" => {
                    let lit: syn::LitStr = content.parse()?;
                    store = Some(lit.value());
                }
                "version" => {
                    let lit: syn::LitStr = content.parse()?;
                    version = Some(lit.value());
                }
                "description" => {
                    let lit: syn::LitStr = content.parse()?;
                    description = Some(lit.value());
                }
                "level" => {
                    let ident: Ident = content.parse()?;
                    level = Some(SpectraLevelSpec::from_ident(&ident)?);
                }
                "default_sample_rate" => {
                    let lit: LitFloat = content.parse()?;
                    default_sample_rate = Some(lit.base10_parse::<f64>()?);
                }
                "coalesce_ms" => {
                    let lit: LitInt = content.parse()?;
                    coalesce_ms = Some(lit.base10_parse::<u64>()?);
                }
                "fields" => {
                    let fields_content;
                    syn::bracketed!(fields_content in content);
                    fields = parse_fields(&fields_content)?;
                }
                _ => {
                    let _ = content.parse::<syn::Expr>();
                    if content.peek(Token![,]) {
                        content.parse::<Token![,]>()?;
                    }
                }
            }
            if content.peek(Token![,]) {
                content.parse::<Token![,]>()?;
            }
        }

        if coalesce_ms.is_some() {
            return Err(syn::Error::new(
                schema_name.span(),
                "`coalesce_ms` is only valid on gauge metrics (`spectra_metric!`); \
                 events use level + default_sample_rate only",
            ));
        }

        Ok(EventSchemaSpec {
            schema_name: schema_name.to_string(),
            table: table.ok_or_else(|| syn::Error::new(schema_name.span(), "missing `table:`"))?,
            store,
            version: version
                .ok_or_else(|| syn::Error::new(schema_name.span(), "missing `version:`"))?,
            description,
            level,
            default_sample_rate,
            fields,
        })
    }
}

impl Parse for MetricSchemaSpec {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let schema_name: Ident = input.parse()?;
        let content;
        braced!(content in input);
        let mut name = None;
        let mut store = None;
        let mut version = None;
        let mut description = None;
        let mut level = None;
        let mut default_sample_rate = None;
        let mut coalesce_ms = None;

        while !content.is_empty() {
            let key: Ident = content.parse()?;
            content.parse::<Token![:]>()?;
            match key.to_string().as_str() {
                "name" => {
                    let lit: syn::LitStr = content.parse()?;
                    name = Some(lit.value());
                }
                "store" => {
                    let lit: syn::LitStr = content.parse()?;
                    store = Some(lit.value());
                }
                "version" => {
                    let lit: syn::LitStr = content.parse()?;
                    version = Some(lit.value());
                }
                "description" => {
                    let lit: syn::LitStr = content.parse()?;
                    description = Some(lit.value());
                }
                "level" => {
                    let ident: Ident = content.parse()?;
                    level = Some(SpectraLevelSpec::from_ident(&ident)?);
                }
                "default_sample_rate" => {
                    let lit: LitFloat = content.parse()?;
                    default_sample_rate = Some(lit.base10_parse::<f64>()?);
                }
                "coalesce_ms" => {
                    let lit: LitInt = content.parse()?;
                    coalesce_ms = Some(lit.base10_parse::<u64>()?);
                }
                _ => {
                    let _ = content.parse::<syn::Expr>();
                }
            }
            if content.peek(Token![,]) {
                content.parse::<Token![,]>()?;
            }
        }

        Ok(MetricSchemaSpec {
            schema_name: schema_name.to_string(),
            name: name.ok_or_else(|| syn::Error::new(schema_name.span(), "missing `name:`"))?,
            store,
            version: version
                .ok_or_else(|| syn::Error::new(schema_name.span(), "missing `version:`"))?,
            description,
            level,
            default_sample_rate,
            coalesce_ms,
        })
    }
}

fn parse_fields(input: ParseStream) -> syn::Result<Vec<FieldSpec>> {
    let mut fields = Vec::new();
    while !input.is_empty() {
        let name: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let field_content;
        braced!(field_content in input);
        let mut rust_type = "String".to_string();
        let mut pii = false;
        let mut safe_for_console = true;

        while !field_content.is_empty() {
            let key: Ident = field_content.parse()?;
            field_content.parse::<Token![:]>()?;
            match key.to_string().as_str() {
                "type" | "r#type" => {
                    if let Ok(ident) = field_content.parse::<Ident>() {
                        rust_type = ident.to_string();
                    } else {
                        let _ = field_content.parse::<syn::Expr>();
                    }
                }
                "classification" => {
                    let class_content;
                    braced!(class_content in field_content);
                    while !class_content.is_empty() {
                        let ck: Ident = class_content.parse()?;
                        class_content.parse::<Token![:]>()?;
                        match ck.to_string().as_str() {
                            "pii" => {
                                let lit: syn::LitBool = class_content.parse()?;
                                pii = lit.value;
                            }
                            "safe_for_console" => {
                                let lit: syn::LitBool = class_content.parse()?;
                                safe_for_console = lit.value;
                            }
                            _ => {
                                let _ = class_content.parse::<syn::Expr>();
                            }
                        }
                        if class_content.peek(Token![,]) {
                            class_content.parse::<Token![,]>()?;
                        }
                    }
                }
                _ => {
                    let _ = field_content.parse::<syn::Expr>();
                }
            }
            if field_content.peek(Token![,]) {
                field_content.parse::<Token![,]>()?;
            }
        }

        fields.push(FieldSpec {
            name: name.to_string(),
            rust_type,
            pii,
            safe_for_console,
        });

        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
    }
    Ok(fields)
}
