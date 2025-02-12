# Distri - TODO List

This TODO list outlines ongoing tasks, feature enhancements, and issues within the Distri framework. Whether you're a first-time contributor or experienced developer, feel free to contribute to any of the tasks listed here.

## Immediate list
- (Done, but maintain) neat logger functionality in distry error -> config file for log output location?
- DB syncing strategy revisit
  - rethink data structures for the DB? something that integrates the merging within, and the metadata.
  - Update collection metadata everywhere to maintain it. 
  - use merge strategy, deletions update metadata "removed bool", compare version_numbers then UTC timestamps
  - when adding a doc, if its UUID is removed, then update it, and all metadata.
  - exchange metadata, then actual data for networking efficiency.
  - sync, send what i have (metadata), the other end does the merging and asks for what it NEEDS.

- error handling in client, raising errors received to client side, handling all cases

## collection of Contents
1. [Bug Fixes](#bug-fixes)
2. [Features to Implement](#features-to-implement)
3. [Enhancements & Improvements](#enhancements--improvements)
4. [Documentation](#documentation)
5. [Testing & Coverage](#testing--coverage)
6. [Help Wanted](#help-wanted)
7. [Roadmap](#roadmap)

---

## Bug Fixes

- **[Bug #101]**: Fix the issue where multiple OK responses cause timeouts in the encrypt test with more than 5 servers. Find a better solution than the hardcoded 5 possible unexpected messages.
  - **Priority**: High
  - **Skills Needed**: Rust, Networking
  - **Status**: Open
  - **Description**: Adjust the packet receive logic to prevent timing out due to excessive OK responses during the election process.

- **[Bug #102]**: Make sure in recv_reliable and all receiving mechanisms the received packets are from the expected sender.
  - **Priority**: High
  - **Skills Needed**: Rust, Networking
  - **Status**: Open
  - **Description**: Adjust the recv_reliable logic (and any receiving logic) to prevent possible bugs or security vulnerabilites with packets from unexpected senders.

---

## Features to Implement
- **[Feature #201]**: Implement sender whitelisting for internal services (related to #202).
  - **Priority**: High
  - **Skills Needed**: Rust, Networking, Security
  - **Status**: Open
  - **Description**: Internal services should be available only to CloudNodes. Implement sender validation based on a whitelist for added security.

- **[Feature #202]**: Protocol for new CloudNodes joining the distributed cloud using secure keys.
  - **Priority**: Medium
  - **Skills Needed**: Rust, Networking, Security
  - **Status**: Open
  - **Description**: Implement a key-pair authentication mechanism for new CloudNodes joining the network. Also, create an exchange of node tables, DB, and available services data.

- **[Feature #203]**: Implement DB Collection deletion.
  - **Priority**: High
  - **Skills Needed**: Rust, Database Management
  - **Status**: Open
  - **Description**: Add functionality to delete collections from the distributed database. This will allow the system to handle dynamic changes in cloud data.

- **[Feature #204]**: Implement standard error responses in CloudNode's service requests.
  - **Priority**: High
  - **Skills Needed**: Rust, Error Handling
  - **Status**: Open
  - **Description**: Ensure all errors in CloudNode are handled with standardized responses. Error types should be clear and actionable by client middleware.

- **[Feature #205]**: Add varying levels of access rights with cloud interactions, .
  - **Priority**: Low
  - **Skills Needed**: Rust, Security
  - **Status**: Open
  - **Description**: Implement mechanisms for securely assigning and identifying access rights of a client, and allow definition of restrictions for services available.


---

## Enhancements & Improvements
- **[Enhancement #301]**: Add pagination to `ReqMem: ReadCollection`.
  - **Priority**: High
  - **Skills Needed**: Rust, Database Management
  - **Status**: Open
  - **Description**: Implement pagination to the `ReadCollection` service to handle large collections efficiently.

- **[Enhancement #302]**: Implement merging of DB data on sync instead of overwriting (related to #303).
  - **Priority**: High
  - **Skills Needed**: Rust, Database Management
  - **Status**: Open
  - **Description**: When syncing between nodes, merge DB entries rather than overwriting. This will improve data consistency and prevent loss of data during sync.

- **[Enhancement #303]**: Optimize DB synchronization to exchange only missing data.
  - **Priority**: Medium
  - **Skills Needed**: Rust, Networking, Database
  - **Status**: Open
  - **Description**: Enhance DB sync by sending only UUIDs of entries that differ between nodes, reducing unnecessary data transfer.

---

## Documentation
- **[Docs #401]**: Expand the README with more use cases and examples.
  - **Priority**: Low
  - **Skills Needed**: Documentation, Rust
  - **Status**: Open
  - **Description**: Provide more practical examples and detailed use cases to help users understand how to implement the Distri framework in their projects.

- **[Docs #402]**: Update CloudNode service documentation.
  - **Priority**: Medium
  - **Skills Needed**: Documentation
  - **Status**: Open
  - **Description**: Update the documentation for CloudNode's services, including the new services and improvements.

---

## Testing & Coverage
- **[Test #501]**: Write robust unit tests for cloud services.
  - **Priority**: High
  - **Skills Needed**: Rust, Testing
  - **Status**: Open
  - **Description**: Implement tests for cloud services service.

- **[Test #502]**: Improve database test coverage.
  - **Priority**: High
  - **Skills Needed**: Rust, Testing, Database
  - **Status**: Open
  - **Description**: Increase test coverage for database operations, especially around handling conflicts when syncing data between nodes.

- **[Test #503]**: Write integration tests for the Peer class.
  - **Priority**: Medium
  - **Skills Needed**: Rust, Testing, Peer-to-Peer Networking
  - **Status**: Open
  - **Description**: Develop integration tests that validate the entire workflow of resource sharing between peers.

---

## Help Wanted
- **[General Task #601]**: Help with documentation.
  - **Priority**: Low
  - **Skills Needed**: Documentation
  - **Status**: Open
  - **Description**: Provide detailed documentation for usage.

- **[General Task #602]**: Help with setting up CI/CD pipelines.
  - **Priority**: Medium
  - **Skills Needed**: DevOps, CI/CD
  - **Status**: Open
  - **Description**: Assist with setting up a CI/CD pipeline to automate builds and tests for the Distri framework.

---

## Roadmap
### **Milestone 1: Node Authentication and Joining Protocol**
- **Timeline**: Q1 2024
- **Description**: Implement secure authentication for new CloudNodes and improve node joining protocol.

### **Milestone 2: Enhanced DB Synchronization**
- **Timeline**: Q2 2024
- **Description**: Improve DB synchronization by merging data instead of overwriting, and implement UUID-based data exchange.

### **Milestone 3: Peer-to-Peer Enhancements**
- **Timeline**: Q3 2024
- **Description**: Enhance the peer-to-peer system to handle more complex resource requests and permissions.

### **Milestone 4: Performance Optimizations**
- **Timeline**: Q4 2024
- **Description**: Focus on performance optimizations, especially for DB operations and handling large-scale distributed systems.

---

## Contribution Guidelines
Please refer to the [CONTRIBUTING.md](link-to-contributing.md) file for detailed contribution instructions.