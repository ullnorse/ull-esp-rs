## Glossary

### OTA Update
Replacing the running firmware by writing a new application image into an inactive OTA application slot, selecting that slot for the next boot, and rebooting the device.

### Bootstrap Flash
The one-time physical flashing step used to install the bootloader, partition table, initial application image, and the minimum configuration needed for the device to join the network and use OTA afterward.

### Application Image
The firmware artifact accepted by the device update flow. It contains only the application image for an OTA application slot, not a full flash image.

### ESP-IDF 2nd-Stage Bootloader
The bootloader model accepted by this repository for OTA behavior, including OTA slot selection and rollback state management.

### Rollback
The behavior where the bootloader returns to the previously working application image if a newly selected application image fails to become confirmed as healthy.

### Device-Hosted Upload OTA
An OTA update flow where the running device exposes an HTTP endpoint and receives the application image directly from a client. The current examples do not implement this flow.

### Device-Pull OTA
An OTA update flow where the device contacts a remote service, determines which application image it should run, downloads that application image itself, and activates it locally.

### Fleet Management Service
A remote service that stores firmware artifacts, tracks device status, decides which application image each device should run, and coordinates device management operations.

### Transfer Integrity Verification
Verification that the received bytes match the intended application image before the device activates the target OTA application slot.

### Health Confirmation
The act of a newly booted application image marking itself as healthy after restoring enough of the device's update path for the product's policy, so the bootloader keeps using that image. In the current `http-ota` example this means restoring network configuration, not proving Fleet Management Service reachability.

### Update Authorization
A policy rule that an OTA request or OTA decision must prove it is allowed to install firmware before the device accepts the application image. The current `http-ota` example does not implement update authorization.

### Update Mode
The device behavior during an OTA transfer where non-essential work is paused so the device focuses on receiving, verifying, and activating a new application image. The current `http-ota` example does not implement a dedicated update mode.
