//! Decodes a synthetic response with the staged archive: exact Arrow types,
//! and a registered column kind that keeps an all-null column typed.
use praevis_thetadata_arrow::arrow_schema::DataType;
use praevis_thetadata_arrow::{ColumnKind, ColumnarDecoder, Limits};
use praevis_thetadata_proto::endpoints::{
    DataTable, DataValue, DataValueList, Price, ResponseData, data_value::DataType as Wire,
};
use prost::Message;

fn main() {
    let cell = |data_type| DataValue {
        data_type: Some(data_type),
    };
    let table = DataTable {
        headers: vec!["price".into(), "open".into()],
        data_table: vec![DataValueList {
            values: vec![
                cell(Wire::Price(Price {
                    value: 12345,
                    r#type: 8,
                })),
                cell(Wire::NullValue(0)),
            ],
        }],
    };
    let response = ResponseData {
        compressed_data: table.encode_to_vec(),
        ..Default::default()
    };
    let mut decoder = ColumnarDecoder::new(Limits::default())
        .unwrap()
        .with_column_kinds(&[("open", ColumnKind::Text)]);
    decoder.push(&response).unwrap();
    let batch = decoder.flush().unwrap();
    assert_eq!(batch.schema().field(0).data_type(), &DataType::Decimal128(38, 10));
    assert_eq!(batch.schema().field(1).data_type(), &DataType::Utf8);
    println!("arrow-consumer: {} row, exact decimal and a typed all-null column", batch.num_rows());
}
