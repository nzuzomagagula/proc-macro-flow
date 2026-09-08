use syn::Type;

//FIXME[ ](#i-was-busy/finish-this):I[this, "The rest of this impleemntation of the syntax"]
pub struct SyntaxExtractor {
    field: SyntaxFieldExtractor,
}

pub struct SyntaxFieldExtractor {
    attribute_kind: AttributeKind,
    syntax_type: Type,
}

pub enum AttributeKind {
    MetaList,
    Path,
    NamedValue,
}

mod scratch {
    #[derive(Syntax)]
    pub struct Configuration {
        //TODO[ ](#attribute/helpers):C[Attr(required), "We can provide the requirement handling as well?"]
        #[metalist]
        colour: ColourSetting,
        #[named_value]
        name: ConfigName,
        #[path]
        no_clean: bool,
    }

    #[derive(Syntax)]
    pub enum ColourSetting {
        Red,
        Black,
        Other(String),
    }

    #[derive(SomeDerive)]
    #[configuration(colour(Red, Other(Blue)), name = "Fuck", NoClean)]
    pub struct Thing;
}
