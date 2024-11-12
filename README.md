# Distri
A Rust framework designed for distributed computing, focusing on both distributed cloud (client-service architecture) and peer-to-peer networking (to be implemented). The framework aims to provide a robust, high-performance, and fault-tolerant solution for building distributed applications with ease.

### TODO
Phase 1: **DONE**
- CMD args for ip tables
- Tolerate failure during handle_connection()
- Usecase on different machines

Phase 2: **(CURRENT)**
- In cloudnode, make file naming unique per socket, add hash prefix to filenames.
- Figure out why Stegnography decryption sometimes fails, maybe add retry feature, or handle the error and count it in failures.
- Add new services to CloudNode for the Directory of service. (Adding data + reading data, in different tables defined by the cloudnode init, JSON formatted) **Ali   (DONE)**
- Figure out syncing the nodes tables (for election conherency, in case a new node joins in & for the Distributed Database) **Ali**
- Adding the permissions to encrypted image data. **Fekry**
- Peer-to-Peer System:
**Fekry**
    - Fetching from directory of service & displaying the list of images, and their owners.
    - Implement requesting and receiving data from peer.
    - Implementing decryption & image viewing at the client.




### Directory Structure:
```
distri/
├── example/         # Example Rust project demonstrating usage of the library
├── tests/           # Folder containing test cases for the library
├── src/             # Main source directory for library modules
│   ├── client.rs    # Client class implementation
│   ├── cloud.rs     # CloudNode implementation
│   ├── utils.rs     # Shared utility functions
│   └── lib.rs       # Library's main access point (entry point for public API)
```

### Testing (Dev)
Test through running the example project in /examples:
```
RUST_BACKTRACE=1 cargo run
```

## Installation

To get started with this framework, ensure you have Rust installed. If you don't have Rust, you can install it from [rust-lang.org](https://www.rust-lang.org/).

Clone the repository:

```bash
git clone https://github.com/yourusername/distributed-computing-framework.git
cd distributed-computing-framework
```
Build the project using Cargo:
```
cargo build
```

## Future Work

•	**Peer-to-Peer Networking**: Implement support for decentralized peer-to-peer communication to enhance the framework’s capabilities.
•	**Enhanced Documentation**: Expand the documentation with more examples and use cases.
•	**Performance Optimizations**: Continuously evaluate and improve the performance of the framework.
•	**Enhanced Customizability**: Better support for seamless integrations with user functions and applications.