use damascene_fonts_inter::{INTER_VARIABLE, INTER_VARIABLE_ITALIC};
use sigilbuzz::Face;
use sigilbuzz_subset::{AxisLimit, UserInstanceInput, instance_user};
use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // GPUI matches registered faces by their static weight.
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    for (style, bytes) in [("", INTER_VARIABLE), ("Italic", INTER_VARIABLE_ITALIC)] {
        let face = Face::parse_bytes(bytes, 0).expect("parse bundled Inter font");
        for (weight, label) in [(400, "Regular"), (600, "SemiBold"), (700, "Bold")] {
            let axes = UserInstanceInput::new().with_axis(*b"wght", AxisLimit::Pin(weight as f32));
            let font = instance_user(&face, &axes).expect("generate Inter weight");
            fs::write(
                output.join(format!("Inter-{style}-{weight}.ttf")),
                named_face(&font.bytes, &format!("{label}{style}")),
            )
            .expect("write Inter weight");
        }
    }
}

// CoreText uses PostScript names to map shaped runs back to the registered face.
fn named_face(bytes: &[u8], style: &str) -> Vec<u8> {
    use write_fonts::{
        FontBuilder,
        from_obj::ToOwnedTable,
        read::{FontRef, TableProvider},
        types::NameId,
    };

    let font = FontRef::new(bytes).expect("read static Inter face");
    let mut names: write_fonts::tables::name::Name =
        font.name().expect("read Inter names").to_owned_table();
    for record in &mut names.name_record {
        let value = match record.name_id {
            NameId::FAMILY_NAME | NameId::TYPOGRAPHIC_FAMILY_NAME => "GPDoc Sans".to_owned(),
            NameId::SUBFAMILY_NAME | NameId::TYPOGRAPHIC_SUBFAMILY_NAME => style.to_owned(),
            NameId::FULL_NAME => format!("GPDoc Sans {style}"),
            NameId::UNIQUE_ID | NameId::POSTSCRIPT_NAME => format!("GPDocSans-{style}"),
            _ => continue,
        };
        record.string = value.into();
    }
    FontBuilder::new()
        .add_table(&names)
        .expect("write Inter names")
        .copy_missing_tables(font)
        .build()
}
