//! Hover, go to definition and completion on small packages, each question asked at a marked
//! place in the text.

use crate::{Completion, CompletionKind, Hover, Session};
use database::File;
use source::Sources;

/// A module of physical constants that other modules import.
const UNITS: &str = "\"\"\"Physical constants in SI units.\"\"\"\n\
                     \n\
                     C: Final = 299792458.0\n\
                     \"\"\"The speed of light, in metres per second.\"\"\"\n\
                     \n\
                     _SECRET: Final = 1\n\
                     \n\
                     \n\
                     def gamma(v: f64) -> f64:\n    \
                         \"\"\"The Lorentz factor of speed `v`.\"\"\"\n    \
                         return 1.0 / (1.0 - (v / C) ** 2) ** 0.5\n";

/// A main module that uses a struct, an enum and the constants module.
const MAIN: &str = "import physics.units\n\
                    from physics.units import gamma\n\
                    \n\
                    \n\
                    class Phase(Enum):\n    \
                        GAS = 1\n    \
                        LIQUID = 2\n\
                    \n\
                    \n\
                    class Body(Struct):\n    \
                        \"\"\"A point mass.\"\"\"\n\
                    \n    \
                        mass: f64\n    \
                        \"\"\"In kilograms.\"\"\"\n    \
                        _tag: i64\n\
                    \n    \
                        def weight(self, g: f64) -> f64:\n        \
                            \"\"\"The force of gravity on the body, in newtons.\"\"\"\n        \
                            return self.mass * g\n\
                    \n\
                    \n\
                    def main():\n    \
                        b = Body(mass=2.0, _tag=0)\n    \
                        w = b.weight(9.81)\n    \
                        p = Phase.GAS\n    \
                        print(w, gamma(0.5), physics.units.C, p == Phase.LIQUID)\n";

/// A session with `physics/units.byn` and `main.byn`; returns the main file.
fn package() -> (Session, File) {
    let mut session = Session::new();
    session.add_file("physics/units.byn", UNITS).unwrap();
    let main = session.add_file("main.byn", MAIN).unwrap();
    (session, main)
}

/// The byte offset of the `occurrence`th (from 0) appearance of `needle` in `text`, plus
/// `within` bytes.
fn at(text: &str, needle: &str, occurrence: usize, within: usize) -> u32 {
    let start = text
        .match_indices(needle)
        .nth(occurrence)
        .map(|(start, _)| start)
        .unwrap();
    u32::try_from(start + within).unwrap()
}

/// The hover at the given place in `main.byn`, as its signature and documentation.
fn hover(needle: &str, occurrence: usize) -> Option<(String, Option<String>)> {
    let (session, main) = package();
    session.hover(main, at(MAIN, needle, occurrence, 1)).map(
        |Hover {
             signature,
             documentation,
             ..
         }| (signature, documentation),
    )
}

/// Where go to definition at the given place in `main.byn` goes: the file name and the text
/// of the span.
fn definition(needle: &str, occurrence: usize) -> Option<(String, String)> {
    let (session, main) = package();
    let span = session.definition(main, at(MAIN, needle, occurrence, 1))?;
    let file = session.file(span.file());
    let text = &file.text()[span.start() as usize..span.end() as usize];
    Some((file.name().to_owned(), text.to_owned()))
}

/// The labels completion offers in `text`, a version of `main.byn`, at `offset`.
fn complete(text: &str, offset: u32) -> Vec<Completion> {
    let mut session = Session::new();
    session.add_file("physics/units.byn", UNITS).unwrap();
    session
        .add_file("physics/heat.byn", "ALPHA: Final = 0.1\n")
        .unwrap();
    let main = session.add_file("main.byn", text).unwrap();
    session.complete(main, offset)
}

/// The labels of `completions`.
fn labels(completions: &[Completion]) -> Vec<&str> {
    completions.iter().map(|item| item.label.as_str()).collect()
}

/// The text of `main.byn` with `line` added at the end of `main`, and the offset at its end.
fn typing(line: &str) -> (String, u32) {
    let text = format!("{MAIN}    {line}");
    let offset = u32::try_from(text.len()).unwrap();
    (text, offset)
}

#[test]
fn hover_shows_a_local_variables_inferred_type() {
    assert_eq!(hover("w = ", 0), Some(("w: f64".to_owned(), None)));
    assert_eq!(hover("b.weight", 0), Some(("b: Body".to_owned(), None)));
    assert_eq!(hover("p ==", 0), Some(("p: Phase".to_owned(), None)));
}

#[test]
fn hover_shows_a_functions_signature_and_documentation_across_modules() {
    assert_eq!(
        hover("gamma(0.5)", 0),
        Some((
            "def gamma(v: f64) -> f64".to_owned(),
            Some("The Lorentz factor of speed `v`.".to_owned())
        ))
    );
}

#[test]
fn hover_shows_a_constant_with_its_type_and_value() {
    assert_eq!(
        hover("C, p", 0),
        Some((
            "C: Final[f64] = 299792458.0".to_owned(),
            Some("The speed of light, in metres per second.".to_owned())
        ))
    );
}

#[test]
fn hover_shows_fields_methods_and_enum_members() {
    assert_eq!(
        hover("weight(9", 0),
        Some((
            "def weight(self, g: f64) -> f64".to_owned(),
            Some("The force of gravity on the body, in newtons.".to_owned())
        ))
    );
    assert_eq!(
        hover("mass * g", 0),
        Some(("mass: f64".to_owned(), Some("In kilograms.".to_owned())))
    );
    assert_eq!(
        hover("GAS\n    print", 0),
        Some(("Phase.GAS".to_owned(), None))
    );
}

#[test]
fn hover_on_a_definitions_own_name_shows_the_definition() {
    assert_eq!(
        hover("Body(Struct)", 0),
        Some((
            "class Body(Struct)".to_owned(),
            Some("A point mass.".to_owned())
        ))
    );
    assert_eq!(
        hover("mass: f64", 0),
        Some(("mass: f64".to_owned(), Some("In kilograms.".to_owned())))
    );
    assert_eq!(
        hover("main()", 0),
        Some(("def main() -> None".to_owned(), None))
    );
}

#[test]
fn hover_shows_modules_and_names_the_language_provides() {
    assert_eq!(
        hover("units.C", 0).map(|(signature, _)| signature),
        Some("module physics.units".to_owned())
    );
    assert_eq!(
        hover("physics.units.C", 0),
        Some(("module physics".to_owned(), None))
    );
    assert_eq!(
        hover("print(", 0).map(|(signature, _)| signature),
        Some("print".to_owned())
    );
}

#[test]
fn hover_off_a_name_shows_nothing() {
    assert_eq!(hover("9.81)", 0), None);
    assert_eq!(hover("\n\n\nclass Phase", 0), None);
}

#[test]
fn go_to_definition_finds_locals_items_and_members_across_modules() {
    let found = |file: &str, text: &str| Some((file.to_owned(), text.to_owned()));
    assert_eq!(definition("w, gamma", 0), found("main.byn", "w"));
    assert_eq!(
        definition("gamma(0.5)", 0),
        found("physics/units.byn", "gamma")
    );
    assert_eq!(definition("C, p", 0), found("physics/units.byn", "C"));
    assert_eq!(definition("units.C", 0), found("physics/units.byn", ""));
    assert_eq!(definition("weight(9", 0), found("main.byn", "weight"));
    assert_eq!(definition("mass * g", 0), found("main.byn", "mass"));
    assert_eq!(definition("LIQUID)", 0), found("main.byn", "LIQUID"));
    assert_eq!(definition("Body(mass", 0), found("main.byn", "Body"));
}

#[test]
fn go_to_definition_of_a_name_the_language_provides_finds_nothing() {
    assert_eq!(definition("print(", 0), None);
    assert_eq!(definition("physics.units.C", 0), None);
}

#[test]
fn completion_after_a_value_offers_its_fields_and_methods() {
    let (text, offset) = typing("b.");
    let offered = complete(&text, offset);
    assert_eq!(labels(&offered), ["mass", "_tag", "weight"]);
    assert_eq!(
        offered[0],
        Completion {
            label: "mass".to_owned(),
            kind: CompletionKind::Field,
            detail: Some("mass: f64".to_owned()),
        }
    );
    // Partly typed: the same members, for the editor to filter.
    let (text, offset) = typing("b.we");
    assert_eq!(labels(&complete(&text, offset)), ["mass", "_tag", "weight"]);
}

#[test]
fn completion_hides_private_members_outside_their_module() {
    let mut session = Session::new();
    session
        .add_file(
            "bodies.byn",
            "class Body(Struct):\n    mass: f64\n    _tag: i64\n",
        )
        .unwrap();
    let text = "from bodies import Body\n\n\ndef f(b: Body) -> f64:\n    return b.\n";
    let main = session.add_file("main.byn", text).unwrap();
    let offset = u32::try_from(text.rfind('.').unwrap() + 1).unwrap();
    assert_eq!(labels(&session.complete(main, offset)), ["mass"]);
}

#[test]
fn completion_after_an_enum_offers_its_members() {
    let (text, offset) = typing("q = Phase.");
    let offered = complete(&text, offset);
    assert_eq!(labels(&offered), ["GAS", "LIQUID"]);
    assert_eq!(offered[0].kind, CompletionKind::EnumMember);
    assert_eq!(offered[0].detail.as_deref(), Some("Phase.GAS"));
}

#[test]
fn completion_after_a_module_offers_its_public_definitions() {
    let (text, offset) = typing("x = physics.units.");
    let offered = complete(&text, offset);
    assert_eq!(labels(&offered), ["C", "gamma"]);
    assert_eq!(offered[0].kind, CompletionKind::Constant);
    assert_eq!(offered[1].kind, CompletionKind::Function);
    assert_eq!(
        offered[1].detail.as_deref(),
        Some("def gamma(v: f64) -> f64")
    );
}

#[test]
fn completion_after_a_package_offers_its_modules() {
    let (text, offset) = typing("x = physics.");
    let offered = complete(&text, offset);
    assert_eq!(labels(&offered), ["heat", "units"]);
    assert_eq!(offered[0].detail.as_deref(), Some("module physics.heat"));
}

#[test]
fn completion_after_a_value_without_members_offers_nothing() {
    let (text, offset) = typing("x = w.");
    assert!(complete(&text, offset).is_empty());
}

#[test]
fn completion_elsewhere_offers_the_names_in_scope() {
    let (text, offset) = typing("x = ");
    let offered = complete(&text, offset);
    let names = labels(&offered);
    // The function's variables first, in the order they are bound.
    assert_eq!(names[..3], ["b", "w", "p"]);
    assert_eq!(offered[1].detail.as_deref(), Some("w: f64"));
    // Then the module's definitions, its imports and the language's names.
    assert_eq!(names[3..7], ["Phase", "Body", "main", "physics"]);
    assert_eq!(names[7], "gamma");
    assert_eq!(
        offered[7].detail.as_deref(),
        Some("def gamma(v: f64) -> f64")
    );
    assert!(names.contains(&"prange"));
    assert!(!names.contains(&"x"));
}

#[test]
fn completion_offers_only_variables_bound_before_the_cursor() {
    let text = "def f(n: i64) -> i64:\n    a = n\n    \n    z = a\n    return z\n";
    let offset = u32::try_from(text.find("    \n").unwrap() + 4).unwrap();
    let offered = complete(text, offset);
    assert_eq!(labels(&offered)[..3], ["n", "a", "f"]);
    assert_eq!(offered[0].detail.as_deref(), Some("n: i64"));
}

/// `period.byn` from the VS Code guide (`docs/guide/vscode.md`, step 6).
const PERIOD: &str = r#""""The period of a pendulum, from a simulation and from the small-angle formula."""

from math import sin, sqrt

G: Final = 9.81
PI: Final = 3.141592653589793


def simulated(length: f64, angle: f64, dt: f64) -> f64:
    """The period, in seconds, of a pendulum `length` metres long released at `angle` radians.

    Steps the equation of motion until the pendulum first reaches the bottom, which takes a
    quarter of a period.
    """
    theta = angle
    omega = 0.0
    time = 0.0
    while theta > 0.0:
        omega -= G / length * sin(theta) * dt
        theta += omega * dt
        time += dt
    return 4.0 * time


def small_angle(length: f64) -> f64:
    """The small-angle period, 2 pi sqrt(L / g), in seconds."""
    return 2.0 * PI * sqrt(length / G)
"#;

/// `main.byn` from the VS Code guide, after the spelling is fixed.
const PENDULUM: &str = r#"import period


def main():
    for i in range(1, 4):
        length = 0.5 * i
        swing = period.simulated(length, 0.1, 0.0001)
        formula = period.small_angle(length)
        print(f"{length:.1f} m: simulated {swing:.3f} s, formula {formula:.3f} s")
"#;

#[test]
fn the_vscode_guide_shows_what_the_editor_answers() {
    let mut session = Session::new();
    session.add_file("period.byn", PERIOD).unwrap();
    let main = session.add_file("main.byn", PENDULUM).unwrap();
    assert!(session.check().is_empty());

    let simulated = at(PENDULUM, "simulated(", 0, 1);
    let hover = session.hover(main, simulated).unwrap();
    assert_eq!(
        hover.signature,
        "def simulated(length: f64, angle: f64, dt: f64) -> f64"
    );
    assert!(hover.documentation.unwrap().starts_with(
        "The period, in seconds, of a pendulum `length` metres long released at `angle` \
             radians.\n"
    ));
    let swing = at(PENDULUM, "swing:", 0, 1);
    assert_eq!(session.hover(main, swing).unwrap().signature, "swing: f64");

    let found = session.definition(main, simulated).unwrap();
    assert_eq!(session.file(found.file()).name(), "period.byn");
    assert_eq!(found.start(), at(PERIOD, "simulated", 0, 0));

    let typed = format!("{PENDULUM}        period.\n");
    let main = session.add_file("typed/main.byn", typed.clone()).unwrap();
    let offset = u32::try_from(typed.len() - 1).unwrap();
    assert_eq!(
        labels(&session.complete(main, offset)),
        ["G", "PI", "simulated", "small_angle"]
    );
}
