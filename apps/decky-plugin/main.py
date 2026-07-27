import asyncio
import json
import os
import urllib.error
import urllib.parse
import urllib.request

import decky


class Plugin:
    def _config_path(self) -> str:
        return os.path.join(
            decky.DECKY_USER_HOME,
            ".config",
            "lan-save-sync",
            "agent.json",
        )

    def _load_config(self) -> dict:
        with open(self._config_path(), "r", encoding="utf-8") as handle:
            return json.load(handle)

    def _agent_url(self, config: dict) -> str:
        listen = config.get("listen", "0.0.0.0:48123")
        port = listen.rsplit(":", 1)[-1]
        return f"http://127.0.0.1:{port}"

    def _request(self, method: str, path: str, payload=None):
        config = self._load_config()
        data = None if payload is None else json.dumps(payload).encode("utf-8")
        request = urllib.request.Request(
            self._agent_url(config) + path,
            data=data,
            method=method,
            headers={
                "Authorization": f"Bearer {config['api_token']}",
                "Content-Type": "application/json",
            },
        )
        try:
            with urllib.request.urlopen(request, timeout=1800) as response:
                return json.loads(response.read().decode("utf-8"))
        except urllib.error.HTTPError as error:
            body = error.read().decode("utf-8", errors="replace")
            try:
                message = json.loads(body).get("error", body)
            except json.JSONDecodeError:
                message = body
            raise RuntimeError(message) from error

    async def summary(self):
        try:
            config = self._load_config()
            info = await asyncio.to_thread(self._request, "GET", "/v1/info")
            return {
                "ready": True,
                "device": info["device"],
                "peers": [
                    {"id": peer["id"], "name": peer["name"]}
                    for peer in config.get("peers", [])
                ],
                "folders": [
                    {"id": folder["id"], "name": folder["name"]}
                    for folder in info.get("folders", [])
                    if folder.get("enabled", False)
                ],
            }
        except Exception as error:
            decky.logger.exception("Unable to load LAN Save Sync")
            return {"ready": False, "error": str(error)}

    async def plan(self, peer_id: str, folder_id: str):
        query = urllib.parse.urlencode(
            {"peer_id": peer_id, "folder_id": folder_id}
        )
        return await asyncio.to_thread(
            self._request, "GET", f"/v1/plan?{query}"
        )

    async def sync(
        self,
        peer_id: str,
        folder_id: str,
        action: str,
        accept_conflict: bool,
    ):
        return await asyncio.to_thread(
            self._request,
            "POST",
            "/v1/sync",
            {
                "peer_id": peer_id,
                "folder_id": folder_id,
                "action": action,
                "accept_conflict": accept_conflict,
            },
        )

    async def _main(self):
        decky.logger.info("LAN Save Sync Decky interface loaded")

    async def _unload(self):
        decky.logger.info("LAN Save Sync Decky interface unloaded")

    async def _uninstall(self):
        # The Agent, configuration, and save history are deliberately managed
        # by the separate installer so removing a UI cannot delete user data.
        decky.logger.info("LAN Save Sync Decky interface removed; Agent data kept")
