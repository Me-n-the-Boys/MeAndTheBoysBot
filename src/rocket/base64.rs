use base64::Engine;

pub struct Base64Decode{
    pub data: Vec<u8>
}

impl<'a> rocket::form::FromFormField<'a> for Base64Decode{
    fn from_value(field: rocket::form::ValueField<'a>) -> rocket::form::Result<'a, Self> {
        let data = match base64::engine::general_purpose::URL_SAFE.decode(field.value.as_bytes()){
            Ok(data) => data,
            Err(err) => return Err(rocket::form::Error::validation(format!("Invalid base64 data: {err}")).into())
        };
        Ok(Base64Decode{data})
    }

}