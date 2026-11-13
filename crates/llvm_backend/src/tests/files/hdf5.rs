//! Calls of the `h5by` module's runtime functions (decision 0052): the file's description, the
//! deflated chunks a read needs inflated, and a dataset's values read.

use super::{Argument, ArrayFile, Database, Float, Integer, LocalKind, Primitive, Type, TypeKind};
use super::{
    ConstantValue, TerminatorKind, constant, end, fits_body, fits_call, module, primitive, span,
};

/// The four words of the subscript item `1`, an index, as `int` arguments.
fn index_item(database: &Database) -> [Argument<'_>; 4] {
    let int = primitive(database, Primitive::Integer(Integer::I64));
    [0, 1, i64::MIN, i64::MIN].map(|value| constant(int, ConstantValue::Integer(value.into())))
}

/// `File("in.h5")`, then the deflated chunks of `temp[1]`: the chunks call passes the
/// dataset's path and the subscript's items after the file's path, as a `NetCDF` read does.
#[test]
fn hdf5_chunks_pass_the_dataset_and_the_items() {
    let database = Database::default();
    let (mut body, [lines, _, error]) = fits_body(&database);
    let str = Type::new(&database, TypeKind::Str);
    let text = |text: &str| constant(str, ConstantValue::Str(text.to_owned()));
    let open = ArrayFile::Hdf5Open;
    body.push_block(fits_call(open, vec![text("in.h5")], lines, error, (1, 3)));
    let mut arguments = vec![text("in.h5"), text("temp")];
    arguments.extend(index_item(&database));
    let chunks = ArrayFile::Hdf5Chunks;
    body.push_block(fits_call(chunks, arguments, lines, error, (2, 3)));
    body.push_block(end(TerminatorKind::Return));
    body.push_block(end(TerminatorKind::Return));
    assert_eq!(
        module(&database, &[("chunks", body)]),
        "\
define void @chunks() {
start:
  %t10 = alloca [4 x i64]
  %_1 = alloca { ptr, i64, i64 }
  %_2 = alloca { ptr, [2 x i64], [2 x i64] }
  %_3 = alloca { i8, [1 x i64] }
  br label %bb0
bb0:
  %t0 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775803 }, 0
  %t1 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775803 }, 1
  %t2 = and i64 %t1, 9223372036854775807
  %t3 = call zeroext i1 @baryon_rt_hdf5_open(ptr %t0, i64 %t2, ptr %_1, ptr %_3)
  br i1 %t3, label %bb3, label %bb1
bb1:
  %t4 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775803 }, 0
  %t5 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775803 }, 1
  %t6 = and i64 %t5, 9223372036854775807
  %t7 = extractvalue { ptr, i64 } { ptr @baryon.str.1, i64 -9223372036854775804 }, 0
  %t8 = extractvalue { ptr, i64 } { ptr @baryon.str.1, i64 -9223372036854775804 }, 1
  %t9 = and i64 %t8, 9223372036854775807
  %t11 = getelementptr inbounds [4 x i64], ptr %t10, i64 0, i64 0
  store i64 0, ptr %t11
  %t12 = getelementptr inbounds [4 x i64], ptr %t10, i64 0, i64 1
  store i64 1, ptr %t12
  %t13 = getelementptr inbounds [4 x i64], ptr %t10, i64 0, i64 2
  store i64 -9223372036854775808, ptr %t13
  %t14 = getelementptr inbounds [4 x i64], ptr %t10, i64 0, i64 3
  store i64 -9223372036854775808, ptr %t14
  %t15 = call zeroext i1 @baryon_rt_hdf5_chunks(ptr %t4, i64 %t6, ptr %t7, i64 %t9, ptr %t10, \
i64 1, ptr %_1, ptr %_3)
  br i1 %t15, label %bb3, label %bb2
bb2:
  ret void
bb3:
  ret void
}

@baryon.str.0 = private unnamed_addr constant [5 x i8] c\"in.h5\"
@baryon.str.1 = private unnamed_addr constant [4 x i8] c\"temp\"

declare zeroext i1 @baryon_rt_hdf5_chunks(ptr, i64, ptr, i64, ptr, i64, ptr, ptr) nounwind
declare zeroext i1 @baryon_rt_hdf5_open(ptr, i64, ptr, ptr) nounwind
"
    );
}

/// The values of `temp[1]` read into an `Array[f64, 1]`: the read passes the items, then the
/// headers of the inflated chunks and of their sizes, then the element number and the rank.
#[test]
fn hdf5_reads_pass_the_inflated_chunks_and_their_sizes() {
    let database = Database::default();
    let (mut body, [_, _, error]) = fits_body(&database);
    let float = primitive(&database, Primitive::Float(Float::F64));
    let byte = primitive(&database, Primitive::Integer(Integer::U8));
    let int = primitive(&database, Primitive::Integer(Integer::I64));
    let rank_one = |element| Type::new(&database, TypeKind::Array { element, rank: 1 });
    let values = body.push_local(rank_one(float), LocalKind::Temporary, span());
    let inflated = body.push_local(rank_one(byte), LocalKind::Temporary, span());
    let sizes = body.push_local(rank_one(int), LocalKind::Temporary, span());
    let str = Type::new(&database, TypeKind::Str);
    let text = |text: &str| constant(str, ConstantValue::Str(text.to_owned()));
    let mut arguments = vec![
        text("in.h5"),
        text("temp"),
        Argument::Borrow(inflated.into()),
        Argument::Borrow(sizes.into()),
    ];
    arguments.extend(index_item(&database));
    let read = ArrayFile::Hdf5Read;
    body.push_block(fits_call(read, arguments, values, error, (1, 2)));
    body.push_block(end(TerminatorKind::Return));
    body.push_block(end(TerminatorKind::Return));
    assert_eq!(
        module(&database, &[("read", body)]),
        "\
define void @read() {
start:
  %t6 = alloca [4 x i64]
  %_1 = alloca { ptr, i64, i64 }
  %_2 = alloca { ptr, [2 x i64], [2 x i64] }
  %_3 = alloca { i8, [1 x i64] }
  %_4 = alloca { ptr, [1 x i64], [1 x i64] }
  %_5 = alloca { ptr, [1 x i64], [1 x i64] }
  %_6 = alloca { ptr, [1 x i64], [1 x i64] }
  br label %bb0
bb0:
  %t0 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775803 }, 0
  %t1 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775803 }, 1
  %t2 = and i64 %t1, 9223372036854775807
  %t3 = extractvalue { ptr, i64 } { ptr @baryon.str.1, i64 -9223372036854775804 }, 0
  %t4 = extractvalue { ptr, i64 } { ptr @baryon.str.1, i64 -9223372036854775804 }, 1
  %t5 = and i64 %t4, 9223372036854775807
  %t7 = getelementptr inbounds [4 x i64], ptr %t6, i64 0, i64 0
  store i64 0, ptr %t7
  %t8 = getelementptr inbounds [4 x i64], ptr %t6, i64 0, i64 1
  store i64 1, ptr %t8
  %t9 = getelementptr inbounds [4 x i64], ptr %t6, i64 0, i64 2
  store i64 -9223372036854775808, ptr %t9
  %t10 = getelementptr inbounds [4 x i64], ptr %t6, i64 0, i64 3
  store i64 -9223372036854775808, ptr %t10
  %t11 = call zeroext i1 @baryon_rt_hdf5_read(ptr %t0, i64 %t2, ptr %t3, i64 %t5, ptr %t6, i64 \
1, ptr %_5, ptr %_6, i32 10, i64 1, ptr %_4, ptr %_3)
  br i1 %t11, label %bb2, label %bb1
bb1:
  ret void
bb2:
  ret void
}

@baryon.str.0 = private unnamed_addr constant [5 x i8] c\"in.h5\"
@baryon.str.1 = private unnamed_addr constant [4 x i8] c\"temp\"

declare zeroext i1 @baryon_rt_hdf5_read(ptr, i64, ptr, i64, ptr, i64, ptr, ptr, i32, i64, ptr, \
ptr) nounwind
"
    );
}
