# Discovered Issues and API Gaps

- [ ] Evaluate and Remove Deprecated functions (Make them internal only where needed for testing)
- [ ] Add protocol Macro that generates a protocol implementation but not a full service
- [ ] EndpointBuilder::with_tcp_listener should align with Endpoint::add_listener
- [ ] Identify if get channels needs a protocol name or can it automatically generate based on protocol
- [ ] EndpointBuilder::with_responder or compantion should accept a trait object to respond with
- [ ] EndpointBuilder::with_responder should accept a protocol Implementation
- [ ] EndpointBuilder should have a method for specifying an on_accept handler (when a listener accepts a new connection)
- [ ] EndpointBuilder should have a method for specifying an on_channel_request handler
- [ ] Create Example of manual construction of transport, channel, and communication with service and message protocol
- [ ] Create Example of endpoint construction with service and message protocol

