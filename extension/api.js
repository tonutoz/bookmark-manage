// 앱(src-tauri/src/extension_server.rs)의 PORT 와 같은 값이어야 한다.
const BASE = "http://127.0.0.1:43917";

const api = {
  async token() {
    return (await chrome.storage.local.get("token")).token || "";
  },

  async pending() {
    return (await chrome.storage.local.get("pending")).pending || [];
  },

  /** 앱이 응답했는데 거절한 경우 err.status 가 있고, 앱이 꺼져 있으면 status 없이 던진다. */
  async request(method, path, body) {
    const response = await fetch(BASE + path, {
      method,
      headers: {
        Authorization: `Bearer ${await this.token()}`,
        ...(body ? { "Content-Type": "application/json" } : {}),
      },
      body: body ? JSON.stringify(body) : undefined,
    });
    const data = await response.json().catch(() => ({}));
    if (!response.ok) {
      const error = new Error(data.error || `HTTP ${response.status}`);
      error.status = response.status;
      throw error;
    }
    return data;
  },

  ping() {
    return this.request("GET", "/ping");
  },

  /** 앱의 워크스페이스 목록: { active, items: [{ id, name }] } */
  workspaces() {
    return this.request("GET", "/workspaces");
  },

  /** 저장 위치로 고른 워크스페이스의 id. 고른 적이 없으면 빈 문자열이고, 그러면 앱에서 열려 있는 곳에 저장된다. */
  async workspace() {
    return (await chrome.storage.local.get("workspace")).workspace || "";
  },

  /** 요청 내용에 저장 위치를 붙인다. */
  async placed(body) {
    const workspace = await this.workspace();
    return workspace ? { ...body, workspace } : body;
  },

  /** 이 주소가 이미 저장돼 있으면 그 북마크를, 없으면 null 을 돌려준다. */
  async lookup(url) {
    return (await this.request("POST", "/lookup", await this.placed({ url }))).bookmark;
  },

  /** 이미 저장된 북마크를 고친다. item 에 id 가 있어야 한다. */
  async update(item) {
    return (await this.request("POST", "/update", await this.placed(item))).bookmark;
  },

  /** 저장한다. 앱이 꺼져 있으면 보관해 두고 { sent: false } 를 돌려준다. */
  async save(item) {
    // 보관했다가 나중에 보내더라도 저장할 때 고른 곳으로 가도록 지금 붙여 둔다.
    item = await this.placed(item);
    try {
      return { sent: true, ...(await this.request("POST", "/bookmarks", item)) };
    } catch (error) {
      if (error.status) throw error;
      const pending = (await this.pending()).filter((p) => p.url !== item.url);
      pending.push(item);
      await chrome.storage.local.set({ pending });
      return { sent: false };
    }
  },

  /** 보관해 둔 것을 순서대로 보낸다. 남은 개수를 돌려준다. */
  async flush() {
    const pending = await this.pending();
    if (pending.length === 0) return 0;
    let sent = 0;
    for (const item of pending) {
      try {
        await this.request("POST", "/bookmarks", item);
      } catch (error) {
        // 앱이 꺼져 있거나 토큰이 틀리면 나중에 다시 시도한다. 그 외 거절(잘못된 주소 등)은 버린다.
        if (!error.status || error.status === 401 || error.status === 403) break;
      }
      sent += 1;
    }
    const left = pending.slice(sent);
    await chrome.storage.local.set({ pending: left });
    return left.length;
  },
};
