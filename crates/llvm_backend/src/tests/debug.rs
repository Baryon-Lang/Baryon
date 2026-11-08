//! Line tables: each function's subprogram, each instruction's location, and the C `main`'s.

use super::{Database, Module, binary, no_structs, primitive, sources, symbol_of};
use mir::BinaryOperator;
use types::{Float, Primitive};

/// With debug information on, `define` names the function's subprogram, every instruction
/// written from a statement carries its line and column, and the metadata closes the module.
/// The C `main` gets an artificial subprogram, so `main` inlined into it keeps its lines.
#[test]
fn instructions_carry_their_source_location() {
    let database = Database::default();
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let body = binary(f64, f64, BinaryOperator::Multiply);
    let symbol = symbol_of(&database);
    let sources = sources();
    let mut module = Module::new(&database, &sources, &no_structs, &symbol);
    module.debug_info("/home/ada/heat");
    module.define("scale", &body, &[]).unwrap();
    module.entry("scale", &[]).unwrap();
    let location = "!DILocation(line: 1, column: 1, scope: !3)";
    let file = "!DIFile(filename: \"heat.byn\", directory: \"/home/ada/heat\")";
    assert_eq!(
        module.finish(),
        format!(
            "\
define double @scale(double %a1, double %a2) !dbg !3 {{
start:
  %_0 = alloca double
  %_1 = alloca double
  %_2 = alloca double
  store double %a1, ptr %_1
  store double %a2, ptr %_2
  br label %bb0
bb0:
  %t0 = load double, ptr %_1, !dbg {location}
  %t1 = load double, ptr %_2, !dbg {location}
  %t2 = fmul double %t0, %t1, !dbg {location}
  store double %t2, ptr %_0, !dbg {location}
  %t3 = load double, ptr %_0, !dbg {location}
  ret double %t3, !dbg {location}
}}

define i32 @main() !dbg !4 {{
start:
  call void @scale(), !dbg !DILocation(line: 0, scope: !4)
  ret i32 0, !dbg !DILocation(line: 0, scope: !4)
}}

!llvm.dbg.cu = !{{!0}}
!llvm.module.flags = !{{!1, !2}}
!0 = distinct !DICompileUnit(language: DW_LANG_Python, file: {file}, producer: \"baryon\", \
isOptimized: true, runtimeVersion: 0, emissionKind: LineTablesOnly)
!1 = !{{i32 7, !\"Dwarf Version\", i32 4}}
!2 = !{{i32 2, !\"Debug Info Version\", i32 3}}
!3 = distinct !DISubprogram(name: \"scale\", scope: {file}, file: {file}, line: 1, \
type: !DISubroutineType(types: !{{}}), scopeLine: 1, spFlags: DISPFlagDefinition | \
DISPFlagOptimized, unit: !0)
!4 = distinct !DISubprogram(name: \"main\", scope: {file}, file: {file}, line: 0, \
type: !DISubroutineType(types: !{{}}), scopeLine: 0, flags: DIFlagArtificial, \
spFlags: DISPFlagDefinition | DISPFlagOptimized, unit: !0)
"
        )
    );
}
