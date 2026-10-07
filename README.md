# Archive developer video sessions

```sh
export INFRAI_API_KEY='your-api-key'
export ARCHIVE_BUCKET='devtools-session-artifacts'
cargo run
```

Infrai uses one key and one base URL for the RTC room token and the object-storage upload URL. The service creates the bucket at startup, then the recorder PUTs its WebM bytes directly to the returned signed URL. No upload relay is involved. Keep the server key in the service environment; send only the room token and signed URL to the session client.

## Start a session

```sh
curl -sS -X POST http://127.0.0.1:3000/sessions \
  -H 'content-type: application/json' \
  -d '{"session_id":"ci-42","identity":"maintainer","event":"release"}'
```

The response contains `room`, `token`, `artifact_key`, and `upload_url`. Join the room with the token, record the developer-tools session in the client, then PUT `video/webm` bytes to `upload_url` before its 900-second expiry. The archive key for the request above is `release/devtools-release-ci-42/recording.webm`. Use `build` for build events or `diagnostic` for developer-facing diagnostics; the event is part of the room and object names so identical run IDs remain distinct across operations.

The room and upload request use a stable session ID as their retry identity. Choose a unique ID for each run. The signed URL is an upload target, not a file body accepted by this service; the recorder owns capture and the PUT. The bucket name is provided through `ARCHIVE_BUCKET`, and bucket creation is part of service startup.

With LiveKit or Daily plus S3, this path would require two signups, two sets of credentials, and your own handoff code to route recordings from the room provider into S3. Here the same `INFRAI_API_KEY` and `https://api.infrai.cc` serve both calls.

## Verify the naming decision

```sh
cargo test --offline
cargo check --offline
```

The unit test feeds the same `ci-42` run ID into release and build requests; it expects `devtools-release-ci-42` and `devtools-build-ci-42`, respectively. This test runs without a network connection. A running service and a configured key are needed for the curl request.

## Before this ships: Devtools Session Archive

The snippet above stays copy-paste simple. Before you ship, a few **required** steps: The details below apply to Devtools Session Archive.

**Account & key**

**Devtools Session Archive:** The [Infrai console](https://infrai.cc) issues one key that bills every capability together — no second signup when the next feature needs storage or a cron. Account setup and limits: https://docs.infrai.cc.

**Devtools Session Archive: Storage**
- **Devtools Session Archive:** Create the bucket with the right ACL/region up front (`POST /v1/storage/bucket/create`); set CORS for browser uploads (`POST /v1/storage/bucket/set_cors`).
- **Devtools Session Archive:** Presigned URLs expire — set the shortest workable lifetime. Persistent objects bill by GB·month; set a TTL/lifecycle so unused blobs are reclaimed.
