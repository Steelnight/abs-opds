# OPDS for ABS

OPDS-Server for ABS (Audiobookshelf) is a working OPDS server that can be used with Audiobookshelf (and was created by a proof of concept). It is designed to work with the Audiobookshelf API and provides a way to access your books via OPDS.

## Features

- [x] OPDS
- [x] Searching
- [x] Pagination
- [x] Multiple Users
- [x] ABS authentication or legacy API authentication
- [x] Books by Author
- [x] Books by Narrator
- [x] Books by Genre/Tags
- [x] Books by Series
- [x] Optional card pagination (A, B, C, ...) instead of author, narrator, etc. names directly.

\*1 If the user is not specified in the ENVs, the system will automatically try to authenticate against ABS.

## Tested with

- [x] Thorium
- [x] Moon+ Reader

## Built-In Demo

Spin up the provided Docker Compose instance and add `http://<local-server-ip>:3010/opds` to your OPDS reader and type in the credentials `demotest` for both username and password.


## ENVs

The following environment variables can be set in a `.env` file or directly in your Docker Compose setup.

| Variable         | Description                                                                 | Default               | Required |
|------------------|-----------------------------------------------------------------------------|-----------------------|----------|
| ABS_URL          | Your Audiobookshelf server URL, e.g. https://audiobooks.dev                |                       | Yes      |
| SHOW_AUDIOBOOKS  | Show audiobooks in the OPDS feed.                                          | false                 | No       |
| SHOW_CHAR_CARDS  | Show character cards (A, B, C, ...) before showing names of author, narrator, etc. | false                 | No       |
| USE_PROXY        | Use a proxy to connect to ABS. If you use the docker network, set this to true to view covers in your reader. Creates potential security risks if someone can read the RAM of the software. | false                 | No       |
| PORT             | The port the OPDS server will run on.                                      | 3010                  | No       |
| OPDS_PAGE_SIZE   | Number of items on each page in the OPDS feed.                             | 20                    | No       |
| OPDS_USERS       | Comma-separated list of users in the format `username:ABS_API_TOKEN:password`. This does NOT need to be your ABS username and password, but values you can freely set to log in with your reader. |                       | No       |
| OPDS_NO_AUTH     | Set to `true` to disable Basic Auth and automatically log in as a specific user. | false                 | No       |
| ABS_NOAUTH_USERNAME | The username to use for automatic login when `OPDS_NO_AUTH` is true.       |                       | Yes (if no-auth) |
| ABS_NOAUTH_PASSWORD | The password to use for automatic login when `OPDS_NO_AUTH` is true.       |                       | Yes (if no-auth) |

## Attribution
Fork of https://github.com/Vito0912/abs-opds - thank you for all your work!

## About

This repository contains a modified version https://github.com/Vito0912/abs-opds intended to integrate OPDS directly into ABS and to work with koreader (by disabling the login). This repo is intented to experiment with Google Jules.

## Docker Compose

See `docker-compose.yml` for an example setup.
