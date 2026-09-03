// Copyright (c) Meta Platforms, Inc. and affiliates.

use std::path::PathBuf;

use typegen::FlowConfig;
use typegen::HackConfig;
use typegen::SharedConfig;
use typegen::TypeGenConfig;
use typegen::TypeGenDeclTrait;
use typegen::TypeGenFile;
use typegen::TypeGenGeneratedType;
use typegen::TypeScriptConfig;

use crate::Address;
use crate::Animal;
use crate::FlowOverriddenConsts;
use crate::HttpMethod;
use crate::OverrideTest;
use crate::PartialConsts;
use crate::Person;
use crate::Point;
use crate::Shape;
use crate::SkipAndOverrideTest;
use crate::SkipTest;
use crate::Thresholds;
use crate::Timelines;
use crate::TrickyConsts;
use crate::Unit;
use crate::User;
use crate::WrappedString;

pub fn get_all_declarations() -> Vec<TypeGenGeneratedType> {
    vec![
        Address::to_type_decl(),
        Person::to_type_decl(),
        User::to_type_decl(),
        Point::to_type_decl(),
        Unit::to_type_decl(),
        WrappedString::to_type_decl(),
        Animal::to_type_decl(),
        Shape::to_type_decl(),
        HttpMethod::to_type_decl(),
        OverrideTest::to_type_decl(),
        SkipTest::to_type_decl(),
        SkipAndOverrideTest::to_type_decl(),
        Timelines::to_type_decl(),
        TrickyConsts::to_type_decl(),
        Thresholds::to_type_decl(),
        PartialConsts::to_type_decl(),
        FlowOverriddenConsts::to_type_decl(),
    ]
}

pub fn gen_config() -> TypeGenConfig {
    TypeGenConfig {
        typescript: Some(TypeScriptConfig {
            shared_config: SharedConfig {
                export_path: Some("./ts".to_string()),
                header: Some("/* ts header */".to_string()),
                file_name_prefix: Some("TSPrefix".to_string()),
                type_name_prefix: Some("TSType".to_string()),
            },
        }),
        flow: Some(FlowConfig {
            shared_config: SharedConfig {
                export_path: Some("./flow".to_string()),
                header: Some("/* flow header */".to_string()),
                file_name_prefix: Some("FlowPrefix".to_string()),
                type_name_prefix: Some("FlowType".to_string()),
            },
        }),
        hack: Some(HackConfig {
            shared_config: SharedConfig {
                export_path: Some("./hack".to_string()),
                header: Some("<?hh\n/* hack header */".to_string()),
                file_name_prefix: Some("HackPrefix".to_string()),
                type_name_prefix: Some("HackType".to_string()),
            },
        }),
        config_file_path: PathBuf::from("typegen_config.json"),
    }
}

pub fn format_types(files: &[TypeGenFile]) -> String {
    files
        .iter()
        .map(|file| {
            format!(
                "---------------- {}\n\n{}",
                file.path.display(),
                file.content
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
