use std::{any::Any, collections::HashMap};

// pub trait Service: Send + Sync {
//     type RequestData;  // Type for the request data
//     type ResponseData; // Type for the response data

//     /// Returns the name of the service
//     fn name(&self) -> &'static str;

//     /// Serializes request args and data
//     fn serialize_request(
//         &self,
//         data: Option<Self::RequestData>,
//     ) -> Result<Vec<u8>, std::io::Error>;

//     fn serialize_response(
//         &self,
//         data: Option<Self::ResponseData>,
//     ) -> Result<Vec<u8>, std::io::Error>;

//     /// Deserializes response data into the expected type
//     fn deserialize_request(&self, data: Vec<u8>) -> Result<Self::RequestData, std::io::Error>;
//     fn deserialize_response(&self, data: Vec<u8>) -> Result<Self::ResponseData, std::io::Error>;

//     /// Processes the args and optionally receives data, returning serialized response
//     fn process(&self, args: HashMap<String, String>, data: Option<Self::RequestData>) -> Result<Self::ResponseData, std::io::Error>;
// }

pub trait Service: Send + Sync {
    fn name(&self) -> &'static str;

    fn serialize_request(&self, data: Box<dyn Any + Send>) -> Result<Vec<u8>, std::io::Error>;

    fn deserialize_request(&self, data: Vec<u8>) -> Result<Box<dyn Any + Send>, std::io::Error>;

    fn serialize_response(&self, data: Box<dyn Any + Send>) -> Result<Vec<u8>, std::io::Error>;

    fn deserialize_response(&self, data: Vec<u8>) -> Result<Box<dyn Any + Send>, std::io::Error>;

    fn process(&self, args: HashMap<String, String>, data: Option<Box<dyn Any + Send>>) -> Result<Box<dyn Any + Send>, std::io::Error>;
}