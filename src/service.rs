use std::collections::HashMap;

pub trait Service: Send + Sync {
    type RequestData;  // Type for the request data
    type ResponseData; // Type for the response data

    /// Returns the name of the service
    fn name(&self) -> &'static str;

    /// Serializes request args and data
    fn serialize_request(
        &self,
        args: Option<HashMap<String, String>>,
        data: Option<Self::RequestData>,
    ) -> Vec<u8>;

    /// Deserializes response data into the expected type
    fn deserialize_response(&self, data: Vec<u8>) -> Self::ResponseData;

    /// Processes the args and optionally receives data, returning serialized response
    fn process(&self, args: serde_json::Value, data: Option<Self::RequestData>) -> Vec<u8>;
}