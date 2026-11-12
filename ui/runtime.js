/* Client for live form sessions: the form's triggers and PL/SQL run on the server. */
window.FormsRuntime = class FormsRuntime {
  constructor() {
    this.sessionId = null;
  }

  static requiresCredentials({ backend, username, password, useServerCredentials = false }) {
    return ['oracle', 'oracle-thin'].includes(backend) && !useServerCredentials &&
      (!username?.trim() || !password);
  }

  get active() {
    return !!this.sessionId;
  }

  async request(method, url, body) {
    const res = await fetch(url, {
      method,
      headers: body ? { 'Content-Type': 'application/json' } : undefined,
      body: body ? JSON.stringify(body) : undefined,
    });
    if (res.status === 204) return null;
    const data = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(data.error || `Request failed (${res.status})`);
    return data;
  }

  /** Starts a session. `backend` may be '' for no database. */
  async start({ moduleName, backend, username, password, allowWrites }) {
    const state = await this.request('POST', '/api/sessions', {
      moduleName,
      backend: backend || null,
      username: username || null,
      password: password || null,
      allowWrites: !!allowWrites,
    });
    this.sessionId = state.sessionId;
    return state;
  }

  async send(event) {
    if (!this.sessionId) throw new Error('No form is running.');
    return this.request('POST', `/api/sessions/${encodeURIComponent(this.sessionId)}/events`, event);
  }

  async answerAlert(button) {
    if (!this.sessionId) throw new Error('No form is running.');
    return this.request('POST', `/api/sessions/${encodeURIComponent(this.sessionId)}/alert`, { button });
  }

  async stop() {
    const id = this.sessionId;
    this.sessionId = null;
    if (id) {
      try {
        await this.request('DELETE', `/api/sessions/${encodeURIComponent(id)}`);
      } catch (_) {
        /* already expired */
      }
    }
  }
};
