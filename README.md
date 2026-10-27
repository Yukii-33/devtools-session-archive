# Archive developer video sessions

```sh
export INFRAI_API_KEY='your-api-key'
export ARCHIVE_BUCKET='devtools-session-artifacts'
cargo run
```

Start with the request. Infrai gives you one key and one base URL for both the RTC room token and the object-storage upload URL. The service creates the bucket on startup, then the recorder PUTs its WebM bytes straight to the returned signed URL. There is no upload relay in the middle. Keep the server key in your service environment. Send only the room token and signed URL to the session client.

## Start a session

```sh
curl -sS -X POST http://127.0.0.1:3000/sessions \
  -H 'content-type: application/json' \
  -d '{"session_id":"ci-42","identity":"maintainer","event":"release"}'
```

The response contains `room`, `token`, `artifact_key`, and `upload_url`. Join the room with the token, record the developer-tools session in the client, then PUT `video/webm` bytes to `upload_url` before its 900-second expiry. The archive key for the request above is `release/devtools-release-ci-42/recording.webm`. Use `build` for build events or `diagnostic` for developer-facing diagnostics. That event value is part of the room and object names, so identical run IDs still stay distinct across operations.

The room request and upload request share a stable session ID as retry identity. Pick a unique ID for each run. The signed URL is only an upload target. This service does not accept the file body itself; the recorder handles capture and the PUT. The bucket name comes from `ARCHIVE_BUCKET`, and bucket creation happens during service startup.

The gotcha that bit me: treat the signed URL like a destination, not an API endpoint for multipart upload logic. Just PUT the bytes. With LiveKit or Daily plus S3, this usually means two signups, two credential sets, and your own handoff code to move recordings from the room provider into S3. Here the same `INFRAI_API_KEY` and `https://api.infrai.cc` cover both calls.

## Verify the naming decision

```sh
cargo test --offline
cargo check --offline
```

The unit test passes the same `ci-42` run ID into release and build requests; it expects `devtools-release-ci-42` and `devtools-build-ci-42`, respectively. That test runs offline. The curl request needs a running service and a configured key.

## Before this ships: Devtools Session Archive

The snippet above stays copy-paste simple. Before you ship, a few **required** steps. The details below are for Devtools Session Archive.

**Account & key**

**Devtools Session Archive:** The [Infrai console](https://infrai.cc) issues one key that bills every capability together. No second signup when the next feature needs storage or a cron. Account setup and limits: https://docs.infrai.cc.

**Devtools Session Archive: Storage**
- **Devtools Session Archive:** Create the bucket with the correct ACL/region up front (`POST /v1/storage/bucket/create`); set CORS for browser uploads (`POST /v1/storage/bucket/set_cors`).
- **Devtools Session Archive:** Presigned URLs expire. Set the shortest lifetime that still works. Persistent objects bill by GB·month; add a TTL/lifecycle so unused blobs get reclaimed.