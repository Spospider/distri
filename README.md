# Distri
A Rust framework designed for distributed computing, focusing on both distributed cloud (client-service architecture) and peer-to-peer networking. The framework aims to provide a robust, high-performance, and fault-tolerant solution for building distributed applications with ease.


## Service Components

### CloudNode
The CloudNode is essentially a worker participating int he cloud. It has a main thread, in-which it receives all incoming requests, then handles most* requests is their own separate threads, providing high availability.

CloudNodes also hold their own NoSQL in-memory DB. its not designed for holding large amounts of data, as this data syncs with all nodes after each data-altering DB operation.

\* Read-write operations on the DB are syncronous in the main thread to guarantee data consistency.

>TODO Add Standard error responses to be handled & reflected in client middlewares.

#### Available Services:
**Internal Services: (should only be used by the CloudNodes among themselves)**
- "**ReqInternal: UpdateInfo**"
    Used to Sync data between server nodes. Data exchanged include node id, load, DB Collections, and DB data version.

**Public Services:**
- "**ReqInternal: Stats**"
    Used to fetch stats data from a server, no election is applied with this request, a server will always respond. No (OK) handshaking mechanism here.
- "**Request: Encrypt**"
    Used to encrypt an image and send the result back.

> ***TODO** sender whitelisting, internal services should only be available to CloudNodes*
> ***TODO** Implement protocol for new CloudNodes joining the Distributed cloud.*
> - Key-pair used to authenticate genuine CloudNodes for security.
> - Implement node table, DB, and available services data exchange.
> (Should new (assumed genuine) nodes introduce new data to the cloud? What usecases can this be usedful?)


**Distributed DB Services:**
NoSQL in-memory DB operations. 
Divided into collections of documents, uniquely identified by and aassignable UUID field. if not assigned a new unique UUID will be generated and returned as the response of the operation.
- "**ReqMem: CreateCollection**"
    Used to add a new Collection to the DB, given data of the Collection name as string.
- "**ReqMem: AddDocument<*collection_name*>**"
    Used to add a new document to a collection in the DB, given the parameter collection_name.
- "**ReqMem: UpdateDocument<*collection_name*>**"
    Used to update an existing document to a collection in the DB, given the parameter collection_name, and a json that will be matched with the target entry. if no entry is found, a new entry is added from the given json anyway.

- "**ReqMem: DeleteDocument<*collection_name*>**"
    Delete document from a collection in the DB, given the parameter collection_name, given data of a JSON-String of attributes and values to exact-match on.
    > ***TODO**: range params*
- "**ReqMem: ReadCollection**"
    Returns the complete list of docs for a collection
    > ***TODO** Pagination*

> ***TODO** DeleteCollection*


### Client
The Client acts as the communication interface to the distriibuted service.

#### Raw Methods
Below are more low-level methods for itneraction with the clouds, here you must be sure of the names of the services you're utilising, and any params that should be sent.
- **send_data**(**data**:Vec<u8>, **service_name**:str)
    Requests service and returns the recieved data.
- **send_data_with_params**(**data**:Vec<u8>, **service_name**:str, **params**: Vec<&str>)
    Requests service with params, returns the recieved data.
- **collect_stats**()
    Performs stats request to all servers and prints each response.

#### DB Operations
##### read_collection(collection_name, option<filter>)
Reads data from a collection with optional filter parameters.

```rust
pub async fn read_collection(
    &self,
    collection_name: &str,
    filter: Option<serde_json::Value>
) -> Result<Vec<u8>, std::io::Error>
```
##### add_document(collection_name, document)
Adds a document to a collection.
```rust
pub async fn add_document(
    &self,
    collection_name: &str,
    document: serde_json::Value
) -> Result<Vec<u8>, std::io::Error>
```
##### create_collection(collection_name)
Creates a new collection (table) in the database.
```rust
pub async fn create_collection(
    &self,
    collection_name: &str
) -> Result<Vec<u8>, std::io::Error>
```
##### update_document(collection_name, document)
Updates an existing document in a collection. Creates a new one if it doesn't exist.
```rust
pub async fn update_document(
    &self,
    collection_name: &str,
    document: serde_json::Value
) -> Result<Vec<u8>, std::io::Error>
```

##### delete_document(collection_name, filter)
Deletes a document from a collection based on a filter.
```rust
pub async fn delete_document(
    &self,
    collection_name: &str,
    filter: serde_json::Value
) -> Result<Vec<u8>, std::io::Error>
```


### Peer
The peer is a resource sharing node of a peer-to-peer system. 
Peers rely on a CloudDB to keep track of all resource transaction data, including:
- Pending resource requests
- Incoming resource requests
- Granted resources
- Provided resources (available for other peers to request)

#### Methods:
* **publish_info()** : checks contents of resources folder, publishes a document of my own address and the list of resources (filenames) + maybe some file metadata to the cloud.
* **fetch_catalog()** : fetches the 'catalog' collection from the cloud, showing all published resources, and their owners peer ids.
* **request_resource(peer_id, resource_name, num_views)** : request resource from peer for a certain number of views.
* **grant_resource(peer_id, resource_name, num_views)** : grants and sends the resource to the other peer, makes sure the grant request wasnt already made.
* **update_permissions(peer_id, resource_name, num_views)** : updates the resource grant permissions to the other peer, publishes it to the cloud and attempts to notify the peer directly.
* **access_resource(resource_name, provider_id)** : provider_id can be the peer's own address for local owned resources.
            // it checks the directory of service first for this resource
            // it decrypts the image, and either returns the raw image data to be supplied to a viewer or pops up the viewer
            // updates the directory of service after viewing
            // if remaining views == 0, delete entry from directory of service. like in grant resource


\*The peer class integrates a client for all interactions with the cloud. 


### Usage Philosophy
All extra logic should be impemented in the main program , the serves should just server, aclients should just be used for communication at the byte level, any processing, decryption etc. should be outside of the client and in the main




### Directory Structure:
```
distri/
├── example/         # Example Rust project demonstrating usage of the library
├── tests/           # Folder containing test cases for the library
├── src/             # Main source directory for library modules
│   ├── client.rs    # Client class implementation
│   ├── cloud.rs     # CloudNode implementation
│   ├── peer.rs      # Peer class implementation
│   ├── utils.rs     # Shared utility functions
│   └── lib.rs       # Library's main access point (entry point for public API)
```


## Installation

To get started with this framework, ensure you have Rust installed. If you don't have Rust, you can install it from [rust-lang.org](https://www.rust-lang.org/).

Clone the repository:

Build the library using Cargo:
```
cargo build
```
To use the Example program
```
cd example
cargo build
```



### Running the Example Project
Test through running the example project in /examples:

#### Args:
`--mode [client | server]`
`--report` (flag for the client only, to produce a stats report at the end of the test)
`--ips [comma separated list of server addresses]` (for servers, the first address will be considered as the address of the new worker, make sure to place the first address in the list accordingly)


#### Creating a client:
```
cargo run -- --mode client --report --ips 127.0.0.1:3000,127.0.0.1:3001
```

#### Creating a Server: (Assuming 2 servers here)
Server 1:
```
cargo run -- --mode server --ips 127.0.0.1:3000,127.0.0.1:3001
```
Server 2:
```
cargo run -- --mode server --ips 127.0.0.1:3001,127.0.0.1:3000
```



## Future Work
•	**Enhanced Documentation**: Expand the documentation with more examples and use cases.
•	**Performance Optimizations**: Continuously evaluate and improve the performance of the framework.
•	**Enhanced Customizability**: Better support for seamless integrations with user functions and applications.



load balancing test:
```
Server Stats:
Requests Recieved: 25000
Accepted Requests: 5410
Completed Tasks: 5123
Server Fails: 0
Total Task Time: 662724.08s
Avg Completion Time: 129.36s

            DB Data: catalog: 0 entries, permissions: 0 entries, users: 0 entries

Received response from 127.0.0.1:60517: Server Stats:
Requests Recieved: 24998
Accepted Requests: 6274
Completed Tasks: 5973
Server Fails: 0
Total Task Time: 681306.92s
Avg Completion Time: 114.06s

            DB Data: catalog: 0 entries, permissions: 0 entries, users: 0 entries

Received response from 127.0.0.1:55830: Server Stats:
Requests Recieved: 24999
Accepted Requests: 5954
Completed Tasks: 5693
Server Fails: 0
Total Task Time: 629499.79s
Avg Completion Time: 110.57s

            DB Data: catalog: 0 entries, permissions: 0 entries, users: 0 entries

Received response from 127.0.0.1:64258: Server Stats:
Requests Recieved: 25000
Accepted Requests: 4958
Completed Tasks: 4701
Server Fails: 0
Total Task Time: 602779.53s
Avg Completion Time: 128.22s

            DB Data: catalog: 0 entries, permissions: 0 entries, users: 0 entries

Test completed: Total failed tasks: 2475
Avg response time: 112.24516040772448
```

Comments on Encrypt test:
- each client does 10 encryption tasks sequentially
- Significant failures start to occur once we surpass 5 servers, as by the nature of our election algorithm, all nodes that receive a request respond with ok, only one will be connected (the first one that responds). and at the same time, in our code, we have a checking mechanism in receiving packets, where if the sender is not the expected sender, it waits for another message, but this only happens until the max number of retries is exhausted (in recv_reliable). once all retries are exhaused, an error is raised. but since there are more servers that number of retries, its possible that the expected packet arrives after a number of "OK"s that surpasses the max retry count. the tests are shoter because of this, as in this case the error is not a timeout error.

- Other errors are due to timeouts in responses. its important to balance the targetted response time, timeouts, and number of servers.


Comments on DB test:
- the test is made of 4 sequential DB operations: AddDoc, ReadDoc, UpdateDoc, ReadDoc.
- to improve db response times, an update time is employed, where the nodes are not allowed to sync for a small period of time after an update, to reduce communication overhead. One node (the last one elected as having the latest data) is the one who's gonna do all BD requests for that period of time.
- but the elected value does eventually get out of sync, and other nodes modify their version of the db.
- when they do sync, the one with the latest DB version overwrites the other, thus this is why the number of entries is less than expected when there are multiple cloudnodes.
- Future work to solve this is to have Db sync through merging instead of overwriting. and also through enhancing the sync by instead of dumping all DB data in the response, it sends the UUIDs it has, the other node then checks and they only exchance data they don't have.
- However in cases where there is no tight data operations our system proves functional and in sync.