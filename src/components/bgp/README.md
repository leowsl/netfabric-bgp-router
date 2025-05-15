# BGP Components

This directory contains the components for implementing Border Gateway Protocol (BGP) functionality. The components are organized into separate modules to maintain a clean and modular architecture. 

## Core Components

- `mod.rs` - Rust module definitions and exports
- `bgp_message.rs` - BGP message types and message handling
- `bgp_config.rs` - BGP configuration structures
- `bgp_rib.rs` - BGP Routing Information Base (RIB) implementation
- `bgp_rib_entry.rs` - BGP Routing Information Base (RIB) entry
- `bgp_bestroute.rs` - BGP Best route selection algorithms and path attributes

Session:
- `bgp_sssession.rs` - BGP Session between two routers.
- `ebgp.rs` - External BGP (eBGP) session handling and peer management
- `ibgp.rs` - Internal BGP (iBGP) session handling and route reflection

## Future Additions

The following components may be added in the future:

- `bgp_attributes.rs` - BGP path attributes and community handling
- `bgp_policy.rs` - Route filtering and policy implementation
- `bgp_metrics.rs` - BGP route metrics and path selection
- `bgp_validation.rs` - BGP route validation and security checks
- `bgp_debug.rs` - Debugging and logging utilities for BGP

## Usage

Each component is designed to be used independently while maintaining clear interfaces between modules. The BGP implementation follows standard BGP specifications and best practices for route handling and path selection.

## Dependencies

- Standard library
- Network protocol utilities

## Testing

Each component should include unit tests and integration tests to ensure proper functionality and interoperability.
