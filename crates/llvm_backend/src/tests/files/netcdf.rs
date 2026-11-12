//! Calls of the `netCDF4` module's runtime functions (decision 0047): the header's lines, a
//! variable's values read and written, and a change to a file.

use super::{Argument, ArrayFile, Database, Integer, Local, LocalKind, Primitive, Type, TypeKind};
use super::{
    ConstantValue, TerminatorKind, constant, end, fits_body, fits_call, module, primitive, span,
};

/// `lines` of `Dataset("in.nc")`, then `variables["temp"][:]` of an `Array[i16, 2]`: the read
/// passes the variable's name after the path, then the element number and the rank, as a FITS
/// read does.
#[test]
fn netcdf_reads_pass_the_variable_name_and_the_element_number() {
    let database = Database::default();
    let (mut body, [lines, _, error]) = fits_body(&database);
    let element = primitive(&database, Primitive::Integer(Integer::I16));
    let array = Type::new(&database, TypeKind::Array { element, rank: 2 });
    let values = body.push_local(array, LocalKind::Temporary, span());
    let str = Type::new(&database, TypeKind::Str);
    let path = || constant(str, ConstantValue::Str("in.nc".to_owned()));
    let open = ArrayFile::NetcdfOpen;
    body.push_block(fits_call(open, vec![path()], lines, error, (1, 3)));
    let arguments = vec![path(), constant(str, ConstantValue::Str("temp".to_owned()))];
    let read = ArrayFile::NetcdfRead;
    body.push_block(fits_call(read, arguments, values, error, (2, 3)));
    body.push_block(end(TerminatorKind::Return));
    body.push_block(end(TerminatorKind::Return));
    assert_eq!(
        module(&database, &[("read", body)]),
        "\
define void @read() {
start:
  %_1 = alloca { ptr, i64, i64 }
  %_2 = alloca { ptr, [2 x i64], [2 x i64] }
  %_3 = alloca { i8, [1 x i64] }
  %_4 = alloca { ptr, [2 x i64], [2 x i64] }
  br label %bb0
bb0:
  %t0 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775803 }, 0
  %t1 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775803 }, 1
  %t2 = and i64 %t1, 9223372036854775807
  %t3 = call zeroext i1 @baryon_rt_netcdf_open(ptr %t0, i64 %t2, ptr %_1, ptr %_3)
  br i1 %t3, label %bb3, label %bb1
bb1:
  %t4 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775803 }, 0
  %t5 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775803 }, 1
  %t6 = and i64 %t5, 9223372036854775807
  %t7 = extractvalue { ptr, i64 } { ptr @baryon.str.1, i64 -9223372036854775804 }, 0
  %t8 = extractvalue { ptr, i64 } { ptr @baryon.str.1, i64 -9223372036854775804 }, 1
  %t9 = and i64 %t8, 9223372036854775807
  %t10 = call zeroext i1 @baryon_rt_netcdf_read(ptr %t4, i64 %t6, ptr %t7, i64 %t9, ptr null, \
i64 0, i32 2, i64 2, ptr %_4, ptr %_3)
  br i1 %t10, label %bb3, label %bb2
bb2:
  ret void
bb3:
  ret void
}

@baryon.str.0 = private unnamed_addr constant [5 x i8] c\"in.nc\"
@baryon.str.1 = private unnamed_addr constant [4 x i8] c\"temp\"

declare zeroext i1 @baryon_rt_netcdf_open(ptr, i64, ptr, ptr) nounwind
declare zeroext i1 @baryon_rt_netcdf_read(ptr, i64, ptr, i64, ptr, i64, i32, i64, ptr, ptr) \
nounwind
"
    );
}

/// `lines` of a change to `out.nc`, then `variables["temp"][:] = values` of an
/// `Array[i16, 2]`: the change passes its line after the path, then the lines' address; the
/// write passes the variable's name, the array's header, its rank and the element number.
#[test]
fn netcdf_writes_pass_the_line_or_the_array() {
    let database = Database::default();
    let (mut body, [lines, _, error]) = fits_body(&database);
    let element = primitive(&database, Primitive::Integer(Integer::I16));
    let array = Type::new(&database, TypeKind::Array { element, rank: 2 });
    let values = body.push_local(array, LocalKind::Temporary, span());
    let str = Type::new(&database, TypeKind::Str);
    let text = |text: &str| constant(str, ConstantValue::Str(text.to_owned()));
    let define = ArrayFile::NetcdfDefine;
    let arguments = vec![text("out.nc"), text("C\t1")];
    body.push_block(fits_call(define, arguments, lines, error, (1, 3)));
    let arguments = vec![
        text("out.nc"),
        text("temp"),
        Argument::Borrow(values.into()),
    ];
    let write = ArrayFile::NetcdfWrite;
    body.push_block(fits_call(write, arguments, Local::RETURN, error, (2, 3)));
    body.push_block(end(TerminatorKind::Return));
    body.push_block(end(TerminatorKind::Return));
    assert_eq!(
        module(&database, &[("write", body)]),
        "\
define void @write() {
start:
  %_1 = alloca { ptr, i64, i64 }
  %_2 = alloca { ptr, [2 x i64], [2 x i64] }
  %_3 = alloca { i8, [1 x i64] }
  %_4 = alloca { ptr, [2 x i64], [2 x i64] }
  br label %bb0
bb0:
  %t0 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775802 }, 0
  %t1 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775802 }, 1
  %t2 = and i64 %t1, 9223372036854775807
  %t3 = extractvalue { ptr, i64 } { ptr @baryon.str.1, i64 -9223372036854775805 }, 0
  %t4 = extractvalue { ptr, i64 } { ptr @baryon.str.1, i64 -9223372036854775805 }, 1
  %t5 = and i64 %t4, 9223372036854775807
  %t6 = call zeroext i1 @baryon_rt_netcdf_define(ptr %t0, i64 %t2, ptr %t3, i64 %t5, ptr %_1, \
ptr %_3)
  br i1 %t6, label %bb3, label %bb1
bb1:
  %t7 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775802 }, 0
  %t8 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775802 }, 1
  %t9 = and i64 %t8, 9223372036854775807
  %t10 = extractvalue { ptr, i64 } { ptr @baryon.str.2, i64 -9223372036854775804 }, 0
  %t11 = extractvalue { ptr, i64 } { ptr @baryon.str.2, i64 -9223372036854775804 }, 1
  %t12 = and i64 %t11, 9223372036854775807
  %t13 = call zeroext i1 @baryon_rt_netcdf_write(ptr %t7, i64 %t9, ptr %t10, i64 %t12, \
ptr null, i64 0, ptr %_4, i64 2, i32 2, ptr %_3)
  br i1 %t13, label %bb3, label %bb2
bb2:
  ret void
bb3:
  ret void
}

@baryon.str.0 = private unnamed_addr constant [6 x i8] c\"out.nc\"
@baryon.str.1 = private unnamed_addr constant [3 x i8] c\"C\\091\"
@baryon.str.2 = private unnamed_addr constant [4 x i8] c\"temp\"

declare zeroext i1 @baryon_rt_netcdf_define(ptr, i64, ptr, i64, ptr, ptr) nounwind
declare zeroext i1 @baryon_rt_netcdf_write(ptr, i64, ptr, i64, ptr, i64, ptr, i64, i32, ptr) \
nounwind
"
    );
}

/// `variables["temp"][1, :]` of an `Array[i16, 1]`: the subscript's two items, four words
/// each, go into a stack array whose address and item count follow the name.
#[test]
fn netcdf_subscripts_pass_their_items_in_a_stack_array() {
    let database = Database::default();
    let (mut body, [_, _, error]) = fits_body(&database);
    let element = primitive(&database, Primitive::Integer(Integer::I16));
    let array = Type::new(&database, TypeKind::Array { element, rank: 1 });
    let values = body.push_local(array, LocalKind::Temporary, span());
    let str = Type::new(&database, TypeKind::Str);
    let int = primitive(&database, Primitive::Integer(Integer::I64));
    let text = |text: &str| constant(str, ConstantValue::Str(text.to_owned()));
    let word = |value: i64| constant(int, ConstantValue::Integer(value.into()));
    let mut arguments = vec![text("in.nc"), text("temp")];
    arguments.extend([0, 1, i64::MIN, i64::MIN].map(word));
    arguments.extend([1, i64::MIN, i64::MIN, i64::MIN].map(word));
    let read = ArrayFile::NetcdfRead;
    body.push_block(fits_call(read, arguments, values, error, (1, 2)));
    body.push_block(end(TerminatorKind::Return));
    body.push_block(end(TerminatorKind::Return));
    assert_eq!(
        module(&database, &[("read", body)]),
        "\
define void @read() {
start:
  %t6 = alloca [8 x i64]
  %_1 = alloca { ptr, i64, i64 }
  %_2 = alloca { ptr, [2 x i64], [2 x i64] }
  %_3 = alloca { i8, [1 x i64] }
  %_4 = alloca { ptr, [1 x i64], [1 x i64] }
  br label %bb0
bb0:
  %t0 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775803 }, 0
  %t1 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775803 }, 1
  %t2 = and i64 %t1, 9223372036854775807
  %t3 = extractvalue { ptr, i64 } { ptr @baryon.str.1, i64 -9223372036854775804 }, 0
  %t4 = extractvalue { ptr, i64 } { ptr @baryon.str.1, i64 -9223372036854775804 }, 1
  %t5 = and i64 %t4, 9223372036854775807
  %t7 = getelementptr inbounds [8 x i64], ptr %t6, i64 0, i64 0
  store i64 0, ptr %t7
  %t8 = getelementptr inbounds [8 x i64], ptr %t6, i64 0, i64 1
  store i64 1, ptr %t8
  %t9 = getelementptr inbounds [8 x i64], ptr %t6, i64 0, i64 2
  store i64 -9223372036854775808, ptr %t9
  %t10 = getelementptr inbounds [8 x i64], ptr %t6, i64 0, i64 3
  store i64 -9223372036854775808, ptr %t10
  %t11 = getelementptr inbounds [8 x i64], ptr %t6, i64 0, i64 4
  store i64 1, ptr %t11
  %t12 = getelementptr inbounds [8 x i64], ptr %t6, i64 0, i64 5
  store i64 -9223372036854775808, ptr %t12
  %t13 = getelementptr inbounds [8 x i64], ptr %t6, i64 0, i64 6
  store i64 -9223372036854775808, ptr %t13
  %t14 = getelementptr inbounds [8 x i64], ptr %t6, i64 0, i64 7
  store i64 -9223372036854775808, ptr %t14
  %t15 = call zeroext i1 @baryon_rt_netcdf_read(ptr %t0, i64 %t2, ptr %t3, i64 %t5, ptr %t6, \
i64 2, i32 2, i64 1, ptr %_4, ptr %_3)
  br i1 %t15, label %bb2, label %bb1
bb1:
  ret void
bb2:
  ret void
}

@baryon.str.0 = private unnamed_addr constant [5 x i8] c\"in.nc\"
@baryon.str.1 = private unnamed_addr constant [4 x i8] c\"temp\"

declare zeroext i1 @baryon_rt_netcdf_read(ptr, i64, ptr, i64, ptr, i64, i32, i64, ptr, ptr) \
nounwind
"
    );
}
