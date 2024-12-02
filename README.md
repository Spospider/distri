# Distri
A Rust framework designed for distributed computing, focusing on both distributed cloud (client-service architecture) and peer-to-peer networking. The framework aims to provide a robust, high-performance, and fault-tolerant solution for building distributed applications with ease.


## Service Components

### CloudNode
The CloudNode is essentially a worker participating int he cloud. It has a main thread, in-which it receives all incoming requests, then handles most* requests is their own separate threads, providing high availability.

CloudNodes also hold their own NoSQL in-memory DB. its not designed for holding large amounts of data, as this data syncs with all nodes after each data-altering DB operation.

\* Read-write operations on the DB are syncronous in the main thread to guarantee data consistency.

#### Available Services:
**Internal Services: (should only be used by the CloudNodes among themselves)**
- "**Request: UpdateInfo**"
    Used to Sync data between server nodes. Data exchanged include node id, load, DB Collections, and DB data version.

**Public Services:**
- "**Request: Stats**"
    Used to fetch stats data from a server, no election is applied with this request, a server will always respond. No (OK) handshaking mechanism here.
- "**Request: Encrypt**"
    Used to encrypt an image and send the result back.

> ***TODO** sender whitelisting, internal services should only be available to CloudNodes*
> ***TODO** Implement protocol for new CloudNodes joining the Distributed cloud.*
> - Key-pair used to authenticate genuine CloudNodes for security.
> - Implement node table, DB, and available services data exchange.
> (Should new (assumed genuine) nodes introduce new data to the cloud? What usecases can this be usedful?)


**Distributed DB Services:**
- "**Request: CreateCollection**"
    Used to add a new Collection to the DB, given data of the Collection name as string.
- "**Request: AddDocument<*collection_name*>**"
    Used to add a new document to a collection in the DB, given the parameter collection_name.
- "**Request: DeleteDocument<*collection_name*>**"
    Delete document from a collection in the DB, given the parameter collection_name, given data of a JSON-String of attributes and values to exact-match on.
    > ***TODO**: range params*
- "**Request: ReadCollection**"
    Returns the complete list of docs for a collection
    > ***TODO** Pagination*

> ***TODO** DeleteCollection*

### Client
The Client acts as the communication module to the distriibuted service. Data going in and out of the client is going to be in the format of bytes.


#### Methods
- **send_data**(**data**:Vec<u8>, **service_name**:str)
    Requests service and returns the recieved data.
- **send_data_with_params**(**data**:Vec<u8>, **service_name**:str, **params**: Vec<&str>)
    Requests service with params, returns the recieved data.
- **collect_stats**()
    Performs stats request to all servers and prints each response.

### Peer (TODO Write this part)
\*The peer class should utilise the client for communication with the cloud. The main purpose of the peer is to communicate with other peers.

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
