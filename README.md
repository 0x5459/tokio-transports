# tokio-transport
tokio-transport provides a series of asynchronous I/O transports for communication between processes, such as pipe, TCP, etc.


## Examples
- ***[pipe_lines_json](./examples/pipe_lines_json.rs)*** - Start a child process to communicate with it via a pipe, split messages by line, and use JSON format to serialize and deserialize messages.
- ***[pipe_length_bincode](./examples/pipe_length_bincode.rs)*** - Start a child process to communicate with it via a pipe, split messages [based on length prefix](https://docs.rs/tokio-util/latest/tokio_util/codec/length_delimited/#getting-started), and use [bincode](https://github.com/bincode-org/bincode) to serialize and deserialize messages.
- ***[tcp_lines_json](./examples/tcp_lines_json.rs)*** - Communicate based on TCP, split messages by line, and use JSON format to serialize and deserialize messages.

#### License

Licensed under either of *Apache License, Version
2.0* or *MIT license* at your option.
