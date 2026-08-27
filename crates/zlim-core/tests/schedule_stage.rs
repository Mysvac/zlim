//! Integration tests for `#[derive(ScheduleStage)]`.

use zlim_core::derive::ScheduleStage;
use zlim_core::schedule::ScheduleStage as ScheduleStageTrait;
use zlim_reflect::derive::TypePath;
use zlim_reflect::path::TypePath as TypePathTrait;

// -----------------------------------------------------------------------------
// Unit struct

#[derive(TypePath, ScheduleStage)]
struct Startup;

#[test]
fn unit_struct_stage_name_is_type_path() {
    assert_eq!(
        Startup.stage_name(),
        <Startup as TypePathTrait>::type_path()
    );
}

// -----------------------------------------------------------------------------
// Data-less enum

#[derive(TypePath, ScheduleStage)]
enum MainStage {
    Update,
    Render,
    Transition,
}

#[test]
fn enum_stage_name_uses_variant() {
    let path = <MainStage as TypePathTrait>::type_path();

    assert_eq!(MainStage::Update.stage_name(), format!("{path}::Update"));
    assert_eq!(MainStage::Render.stage_name(), format!("{path}::Render"));
    assert_eq!(
        MainStage::Transition.stage_name(),
        format!("{path}::Transition")
    );
}
