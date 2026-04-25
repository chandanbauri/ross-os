# ROSS Roadmap: Phase 8 (Networking & Connectivity)

Phase 8 transforms ROSS from an isolated machine into a connected node by introducing network drivers | protocol stacks | and socket programming.

## 1. NIC Initialization
- [ ] Locate the E1000 or RTL8139 controller via PCI.
- [ ] Allocate Rx/Tx ring buffers using the Physical Memory Manager.
- [ ] **Milestone:** The kernel logs a message when a physical Ethernet cable is plugged in or unplugged.

## 2. Ethernet and ARP
- [ ] Write a parser to encapsulate and decapsulate Ethernet frames.
- [ ] Implement an ARP cache to resolve IP addresses to MAC addresses.
- [ ] **Milestone:** ROSS successfully broadcasts an ARP request and processes the router's reply.

## 3. IPv4 and ICMP (Ping)
- [ ] Build the IPv4 header struct and checksum calculator.
- [ ] Implement an ICMP echo request and reply handler.
- [ ] **Milestone:** The `ross-sh` command `ping <gateway_ip>` returns valid round-trip times.

## 4. Socket API
- [ ] Add network file descriptors to the Virtual File System (VFS).
- [ ] Implement UDP datagram transmission.
- [ ] **Milestone:** A user-land process successfully requests a webpage's IP address from a DNS server.
