use serde_json::Value;

// struct CrudMeta {
//     table: String,
//     prime_key: Option<String>,
//     key: Option<String>,
// }

// struct CrudMetaWithBody {
//     table: String,
//     prime_key: Option<String>,
//     key: Option<String>,
//     body: Value,
// }

// enum CRUD {
//     Get(CrudMeta),
//     Put(CrudMetaWithBody),
//     Post(CrudMetaWithBody),
//     Delete(CrudMeta),
// }

// #[macro_export]
// macro_rules! get {
//     ($table: expr, $key: expr, $prime_key: expr) => {};
// }

// fn test() {
//     let get_msg = CRUD::Get(CrudMeta {
//         table: "vtuber".to_owned(),
//         key: None,
//         prime_key: None,
//     });
// }

// impl Message for CRUD {
//     type Result = Result<()>;
// }

#[derive(PartialEq, Clone, Copy, Debug)]
enum CrudType {
    Get,
    Put,
    Post,
    Delete,
}

struct Crud {
    crud_type: CrudType,
    data: Option<Value>,
    table: String,
    prime_key: Option<String>,
}
