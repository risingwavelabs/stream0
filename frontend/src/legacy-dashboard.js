const markup = `
<div id="login-page" class="login-page">
  <div class="login-box">
    <h1>Box0</h1>
    <p>Enter your API key to access the dashboard.</p>
    <div id="login-error" class="login-error"></div>
    <input type="password" id="login-key" placeholder="API key" autocomplete="off">
    <button class="btn btn-primary" style="width:100%" onclick="App.auth.login()">Sign in</button>
  </div>
</div>

<div id="app-layout" class="app-layout hidden">
  <nav class="sidebar">
    <div class="sidebar-logo">Box<span>0</span></div>
    <div class="sidebar-nav">
      <a href="#/tasks" data-page="tasks"><span class="nav-icon">T</span> Tasks</a>
    </div>
    <div class="sidebar-nav" style="border-top:1px solid rgba(255,255,255,0.08);padding-top:8px">
      <a href="#/agents" data-page="agents" style="font-size:13px;opacity:0.7"><span class="nav-icon">A</span> Agents</a>
      <a href="#/machines" data-page="machines" style="font-size:13px;opacity:0.7"><span class="nav-icon">M</span> Machines</a>
      <a href="#/users" data-page="users" style="font-size:13px;opacity:0.7"><span class="nav-icon">U</span> Users</a>
    </div>
    <div class="sidebar-group">
      <label>Workspace</label>
      <div style="display:flex;gap:6px;align-items:center">
        <select id="workspace-select" onchange="App.setWorkspace(this.value)" style="flex:1"></select>
        <a href="#/workspaces" title="Manage workspaces" style="color:var(--text-sidebar);opacity:0.5;font-size:16px;text-decoration:none;padding:2px">&#9881;</a>
      </div>
    </div>
    <div class="sidebar-footer">
      <div class="user-name" id="user-name"></div>
      <button onclick="App.auth.logout()">Sign out</button>
    </div>
  </nav>
  <main class="main-content" id="main-content"></main>
</div>

<div class="toast-container" id="toast-container"></div>
`

export function mountLegacyDashboard(root) {
  root.innerHTML = markup

  const App = {}
  window.App = App

  App.toast = {
    show(msg, type = 'success') {
      const el = document.createElement('div')
      el.className = `toast ${type}`
      el.textContent = msg
      document.getElementById('toast-container').appendChild(el)
      window.setTimeout(() => {
        el.remove()
      }, 4000)
    },
    error(msg) {
      App.toast.show(msg, 'error')
    },
    success(msg) {
      App.toast.show(msg, 'success')
    },
  }

  App.api = {
    key: null,

    headers() {
      const headers = { 'Content-Type': 'application/json' }
      if (App.api.key) headers['X-API-Key'] = App.api.key
      return headers
    },

    request(method, path, body) {
      const opts = { method, headers: App.api.headers() }
      if (body) opts.body = JSON.stringify(body)
      return fetch(path, opts).then((res) => {
        if (res.status === 401) {
          App.auth.logout()
          throw new Error('Unauthorized')
        }
        return res.json().then((data) => {
          if (!res.ok) throw new Error(data.error || 'Request failed')
          return data
        })
      })
    },

    get(path) {
      return App.api.request('GET', path)
    },
    post(path, body) {
      return App.api.request('POST', path, body)
    },
    put(path, body) {
      return App.api.request('PUT', path, body)
    },
    del(path) {
      return App.api.request('DELETE', path)
    },
  }

  App.auth = {
    login() {
      const key = document.getElementById('login-key').value.trim()
      if (!key) return

      App.api.key = key
      App.api
        .get('/workspaces')
        .then((data) => {
          localStorage.setItem('b0_api_key', key)
          App.boot(data)
        })
        .catch(() => {
          document.getElementById('login-error').textContent = 'Invalid API key'
          document.getElementById('login-error').style.display = 'block'
          App.api.key = null
        })
    },

    logout() {
      localStorage.removeItem('b0_api_key')
      localStorage.removeItem('b0_workspace')
      App.api.key = null
      document.getElementById('login-page').classList.remove('hidden')
      document.getElementById('app-layout').classList.add('hidden')
      document.getElementById('login-key').value = ''
      document.getElementById('login-error').style.display = 'none'
    },

    tryRestore() {
      const key = localStorage.getItem('b0_api_key')
      if (!key) return
      App.api.key = key
      App.api
        .get('/workspaces')
        .then((data) => {
          App.boot(data)
        })
        .catch(() => {
          localStorage.removeItem('b0_api_key')
          App.api.key = null
        })
    },
  }

  App.workspaces = []
  App.currentWorkspace = null

  App.boot = function boot(data) {
    App.workspaces = data.workspaces || []
    document.getElementById('login-page').classList.add('hidden')
    document.getElementById('app-layout').classList.remove('hidden')

    const sel = document.getElementById('workspace-select')
    sel.innerHTML = ''
    App.workspaces.forEach((workspace) => {
      const opt = document.createElement('option')
      opt.value = workspace.name
      opt.textContent = workspace.name
      sel.appendChild(opt)
    })

    const saved = localStorage.getItem('b0_workspace')
    if (saved && App.workspaces.some((workspace) => workspace.name === saved)) {
      sel.value = saved
      App.currentWorkspace = saved
    } else if (App.workspaces.length > 0) {
      App.currentWorkspace = App.workspaces[0].name
      sel.value = App.currentWorkspace
    }

    document.getElementById('user-name').textContent = ''
    App.router.start()
  }

  App.setWorkspace = function setWorkspace(name) {
    App.currentWorkspace = name
    localStorage.setItem('b0_workspace', name)
    App.router.navigate(location.hash || '#/tasks')
  }

  App.workspacePath = function workspacePath(path) {
    return `/workspaces/${encodeURIComponent(App.currentWorkspace)}${path}`
  }

  App.router = {
    routes: {
      '/tasks': () => {
        App.tasksPage.render()
      },
      '/machines': () => {
        App.machines.render()
      },
      '/agents': () => {
        App.agentsPage.render()
      },
      '/workspaces': () => {
        App.workspacesPage.render()
      },
      '/users': () => {
        App.usersPage.render()
      },
    },

    start() {
      window.removeEventListener('hashchange', App.router.onHashChange)
      window.addEventListener('hashchange', App.router.onHashChange)
      App.router.onHashChange()
    },

    onHashChange() {
      if (App.tasksPage._boardTimer) {
        clearInterval(App.tasksPage._boardTimer)
        App.tasksPage._boardTimer = null
      }
      if (App.tasksPage._chatTimer) {
        clearInterval(App.tasksPage._chatTimer)
        App.tasksPage._chatTimer = null
      }

      const hash = location.hash || '#/tasks'
      const path = hash.slice(1)

      document.querySelectorAll('.sidebar-nav a').forEach((link) => {
        link.classList.toggle('active', link.getAttribute('href') === hash)
      })

      const parts = path.split('/').filter(Boolean)

      if (parts[0] === 'tasks' && parts[1]) {
        App.tasksPage.render(decodeURIComponent(parts[1]))
        document.querySelectorAll('.sidebar-nav a').forEach((link) => {
          link.classList.toggle('active', link.getAttribute('data-page') === 'tasks')
        })
        return
      }

      if (parts[0] === 'machines' && parts[1]) {
        App.machineDetail.render(decodeURIComponent(parts[1]))
        document.querySelectorAll('.sidebar-nav a').forEach((link) => {
          link.classList.toggle('active', link.getAttribute('data-page') === 'machines')
        })
        return
      }

      if (parts[0] === 'agents' && parts[1]) {
        const agentName = decodeURIComponent(parts[1])
        const threadId = parts[2] ? decodeURIComponent(parts[2]) : null
        App.detail.render(agentName, threadId)
        document.querySelectorAll('.sidebar-nav a').forEach((link) => {
          link.classList.toggle('active', link.getAttribute('data-page') === 'agents')
        })
        return
      }

      const base = `/${parts[0] || 'machines'}`
      const handler = App.router.routes[base]
      if (handler) handler()
      else App.tasksPage.render()
    },

    navigate(hash) {
      location.hash = hash
    },
  }

  function esc(value) {
    if (value == null) return ''
    return String(value)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
  }

  function escAttr(value) {
    if (value == null) return ''
    return String(value)
      .replace(/&/g, '&amp;')
      .replace(/"/g, '&quot;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
  }

  function truncate(value, length = 80) {
    if (!value) return ''
    return value.length > length ? `${value.slice(0, length)}...` : value
  }

  function statusDot(status) {
    return `<span class="status-dot ${esc(status)}"></span>${esc(status)}`
  }

  function timeAgo(dateStr) {
    if (!dateStr) return 'never'
    const date = new Date(dateStr)
    const now = new Date()
    const diff = Math.floor((now - date) / 1000)
    if (diff < 60) return `${diff}s ago`
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
    if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
    return `${Math.floor(diff / 86400)}d ago`
  }

  function setContent(html) {
    document.getElementById('main-content').innerHTML = html
  }

  function showLoading() {
    setContent('<div class="loading"><span class="spinner"></span></div>')
  }

  function contentAsText(content) {
    if (content === null || content === undefined) return ''
    if (typeof content === 'string') return content
    if (typeof content === 'object') {
      if (content.text) return content.text
      if (content.message) return content.message
      return JSON.stringify(content, null, 2)
    }
    return String(content)
  }

  App.webAgent = {
    getId() {
      let id = localStorage.getItem('b0_web_agent')
      if (!id) {
        id = `web-${crypto.randomUUID()}`
        localStorage.setItem('b0_web_agent', id)
      }
      return id
    },

    ensureRegistered() {
      const agentId = App.webAgent.getId()
      return Promise.resolve(agentId)
    },
  }

  App.poll = {
    _timers: {},

    start(threadId, workerName, callback) {
      App.poll.stop(threadId)
      const tick = () => {
        App.api
          .get(App.workspacePath(`/threads/${encodeURIComponent(threadId)}`))
          .then((data) => {
            const msgs = data.messages || []
            if (msgs.length > 0) {
              const last = msgs[msgs.length - 1]
              const lastType = last.msg_type || last.type
              callback(msgs, lastType)
              if (lastType === 'done' || lastType === 'question' || lastType === 'failed') {
                App.poll.stop(threadId)
              }
            }
          })
          .catch(() => {})
      }
      tick()
      App.poll._timers[threadId] = window.setInterval(tick, 3000)
    },

    stop(threadId) {
      if (App.poll._timers[threadId]) {
        clearInterval(App.poll._timers[threadId])
        delete App.poll._timers[threadId]
      }
    },

    stopAll() {
      Object.keys(App.poll._timers).forEach((id) => {
        clearInterval(App.poll._timers[id])
      })
      App.poll._timers = {}
    },
  }

  App.tasksPage = {
    _boardTimer: null,
    _chatTimer: null,
    _selectedTaskId: null,
    _tasks: [],

    render(taskId) {
      App.poll.stopAll()
      if (App.tasksPage._boardTimer) clearInterval(App.tasksPage._boardTimer)
      if (App.tasksPage._chatTimer) clearInterval(App.tasksPage._chatTimer)

      const mc = document.getElementById('main-content')
      mc.innerHTML =
        '<div class="tasks-layout">' +
        '<div class="tasks-chat" id="tasks-chat">' +
        '<div class="tasks-chat-header" id="tasks-chat-header">Select a task</div>' +
        '<div class="tasks-chat-messages" id="tasks-chat-messages">' +
        '<div class="tasks-chat-empty">Select a task from the board, or create a new one.</div>' +
        '</div>' +
        '<div class="tasks-chat-input" id="tasks-chat-input" style="display:none">' +
        '<input type="text" id="tasks-chat-field" placeholder="Send a message..." onkeydown="if(event.key===\'Enter\')App.tasksPage.sendMessage()">' +
        '<button class="btn btn-primary" onclick="App.tasksPage.sendMessage()">Send</button>' +
        '</div>' +
        '</div>' +
        '<div class="tasks-board" id="tasks-board">' +
        '<div class="tasks-board-header">' +
        '<h3>Tasks</h3>' +
        '<button class="btn btn-primary btn-sm" onclick="App.tasksPage.showAdd()">+ Add</button>' +
        '</div>' +
        '<div id="tasks-board-list">Loading...</div>' +
        '</div>' +
        '</div>'

      App.tasksPage._selectedTaskId = taskId || null
      App.tasksPage.loadBoard()
      App.tasksPage._boardTimer = window.setInterval(() => {
        App.tasksPage.loadBoard()
      }, 5000)

      if (taskId) {
        App.tasksPage.selectTask(taskId)
      }
    },

    loadBoard() {
      App.api
        .get(App.workspacePath('/tasks'))
        .then((data) => {
          App.tasksPage._tasks = data.tasks || []
          App.tasksPage.renderBoard()
        })
        .catch(() => {})
    },

    renderBoard() {
      const tasks = App.tasksPage._tasks
      const groups = { running: [], needs_input: [], done: [], failed: [] }
      tasks.forEach((task) => {
        if (groups[task.status]) groups[task.status].push(task)
        else groups.running.push(task)
      })

      let html = ''

      if (groups.running.length) {
        html += '<div class="task-group"><div class="task-group-label">Running</div>'
        groups.running.forEach((task) => {
          html += App.tasksPage.renderCard(task)
        })
        html += '</div>'
      }

      if (groups.needs_input.length) {
        html += '<div class="task-group"><div class="task-group-label">Needs Input</div>'
        groups.needs_input.forEach((task) => {
          html += App.tasksPage.renderCard(task)
        })
        html += '</div>'
      }

      if (groups.done.length) {
        html += '<div class="task-group"><div class="task-group-label">Done</div>'
        groups.done.forEach((task) => {
          html += App.tasksPage.renderCard(task)
        })
        html += '</div>'
      }

      if (groups.failed.length) {
        html += '<div class="task-group"><div class="task-group-label">Failed</div>'
        groups.failed.forEach((task) => {
          html += App.tasksPage.renderCard(task)
        })
        html += '</div>'
      }

      if (!tasks.length) {
        html =
          '<div style="color:var(--text-secondary);font-size:13px;text-align:center;padding:40px 0">No tasks yet. Click + Add to create one.</div>'
      }

      document.getElementById('tasks-board-list').innerHTML = html
    },

    renderCard(task) {
      const selected = App.tasksPage._selectedTaskId === task.id ? ' selected' : ''
      return (
        `<div class="task-card${selected}" onclick="App.tasksPage.selectTask('${escAttr(task.id)}')">` +
        `<div class="task-card-title">${esc(task.title)}</div>` +
        '<div class="task-card-meta">' +
        `<span class="task-status-dot ${esc(task.status)}"></span> ` +
        `${esc(task.status)} &middot; ${timeAgo(task.created_at)}` +
        '</div>' +
        '</div>'
      )
    },

    selectTask(taskId) {
      App.tasksPage._selectedTaskId = taskId
      if (App.tasksPage._chatTimer) clearInterval(App.tasksPage._chatTimer)

      App.tasksPage.renderBoard()
      App.tasksPage.loadTask(taskId)
      App.tasksPage._chatTimer = window.setInterval(() => {
        App.tasksPage.loadTask(taskId)
      }, 3000)
    },

    loadTask(taskId) {
      App.api
        .get(App.workspacePath(`/tasks/${taskId}`))
        .then((data) => {
          App.tasksPage.renderChat(data)
        })
        .catch(() => {})
    },

    renderChat(data) {
      const task = data.task
      const messages = data.messages || []
      const subtasks = data.subtasks || []

      document.getElementById('tasks-chat-header').textContent = task.title
      document.getElementById('tasks-chat-input').style.display = 'flex'

      let html = ''

      messages.forEach((message) => {
        if (message.type === 'started') return

        const isUser = message.type === 'request' || message.type === 'answer'
        const cls = isUser ? 'user' : 'assistant'
        let content = ''
        if (message.content) {
          content = typeof message.content === 'string' ? message.content : JSON.stringify(message.content)
          if (content.startsWith('"') && content.endsWith('"')) {
            try {
              content = JSON.parse(content)
            } catch {
              // Keep original string if not valid JSON.
            }
          }
        }
        if (!content) return

        html +=
          `<div class="chat-msg ${cls}">` +
          `<div class="chat-msg-bubble">${esc(content)}</div>` +
          `<div class="chat-msg-meta">${timeAgo(message.created_at)}</div>` +
          '</div>'
      })

      if (subtasks.length) {
        html +=
          '<div class="subtask-list"><div style="font-size:12px;font-weight:600;color:var(--text-secondary);margin-bottom:4px">Sub-tasks</div>'
        subtasks.forEach((subtask) => {
          html +=
            '<div class="subtask-item">' +
            `<span class="task-status-dot ${esc(subtask.status)}"></span> ` +
            `<span>${esc(subtask.title)}</span>` +
            `<span style="margin-left:auto;font-size:11px;color:var(--text-secondary)">${esc(subtask.status)}</span>` +
            '</div>'
        })
        html += '</div>'
      }

      if (!html) {
        html = '<div class="tasks-chat-empty">Waiting for response...</div>'
      }

      const el = document.getElementById('tasks-chat-messages')
      el.innerHTML = html
      el.scrollTop = el.scrollHeight
    },

    sendMessage() {
      const field = document.getElementById('tasks-chat-field')
      const content = field.value.trim()
      if (!content || !App.tasksPage._selectedTaskId) return
      field.value = ''

      App.api
        .post(App.workspacePath(`/tasks/${App.tasksPage._selectedTaskId}/messages`), { content })
        .then(() => {
          App.tasksPage.loadTask(App.tasksPage._selectedTaskId)
        })
        .catch((error) => {
          App.toast.error(error.message)
        })
    },

    showAdd() {
      const title = prompt('What do you need?')
      if (!title || !title.trim()) return

      App.api
        .post(App.workspacePath('/tasks'), { title: title.trim() })
        .then((task) => {
          App.toast.success('Task created')
          App.tasksPage.loadBoard()
          App.tasksPage.selectTask(task.id)
        })
        .catch((error) => {
          App.toast.error(error.message)
        })
    },
  }

  App.machines = {
    render() {
      App.poll.stopAll()
      showLoading()
      App.api
        .get('/machines')
        .then((data) => {
          const machines = data.machines || []
          const countPromise = App.currentWorkspace
            ? App.api
                .get(App.workspacePath('/agents'))
                .then((agentsData) => agentsData.agents || [])
                .catch(() => [])
            : Promise.resolve([])
          return countPromise.then((agents) => {
            const countByMachine = {}
            const activeByMachine = {}
            agents.forEach((agent) => {
              const machineId = agent.machine_id || 'unknown'
              countByMachine[machineId] = (countByMachine[machineId] || 0) + 1
              if (agent.status === 'active') activeByMachine[machineId] = (activeByMachine[machineId] || 0) + 1
            })
            let html = ''
            html += '<div class="page-header"><h2>Machines</h2>'
            html += '<button class="btn btn-primary" onclick="App.machines.showAdd()">+ Add Machine</button></div>'
            html += '<div class="card"><table>'
            html += '<thead><tr><th>Name</th><th>Status</th><th>Agents</th><th>Last Seen</th><th></th></tr></thead><tbody>'
            if (machines.length === 0) {
              html +=
                '<tr><td colspan="5" style="text-align:center;color:var(--text-secondary);padding:32px">No machines connected yet</td></tr>'
            }
            machines.forEach((machine) => {
              const total = countByMachine[machine.id] || 0
              const active = activeByMachine[machine.id] || 0
              html += `<tr class="clickable" onclick="App.router.navigate('#/machines/${encodeURIComponent(machine.id)}')">`
              html += `<td><strong>${esc(machine.id)}</strong></td>`
              html += `<td>${statusDot(machine.status)}</td>`
              html +=
                `<td>${total > 0 ? `${active} active / ${total} total` : '<span style="color:var(--text-secondary)">0</span>'}</td>`
              html += `<td>${timeAgo(machine.last_heartbeat)}</td>`
              html +=
                `<td><button class="btn btn-sm btn-danger" onclick="event.stopPropagation(); App.machines.remove('${escAttr(machine.id)}')">Remove</button></td>`
              html += '</tr>'
            })
            html += '</tbody></table></div>'
            setContent(html)
          })
        })
        .catch((error) => {
          App.toast.error(`Failed to load machines: ${error.message}`)
        })
    },

    showAdd() {
      let html = '<div class="modal-overlay" onclick="if(event.target===this)this.remove()"><div class="modal">'
      html +=
        '<div class="modal-header">Add Machine<button class="btn-icon" onclick="this.closest(\'.modal-overlay\').remove()">&times;</button></div>'
      html += '<div class="modal-body">'
      html += '<p style="color:var(--text-secondary);font-size:13px;margin-bottom:16px">To connect a machine, run:</p>'
      html += '<div class="form-group"><label>1. Login to this server</label>'
      html +=
        `<code style="display:block;background:var(--bg);padding:10px 14px;border-radius:var(--radius);font-size:13px;font-family:var(--mono)">b0 login --server ${esc(location.origin)}</code></div>`
      html += '<div class="form-group"><label>2. Start the machine</label>'
      html +=
        '<code style="display:block;background:var(--bg);padding:10px 14px;border-radius:var(--radius);font-size:13px;font-family:var(--mono)">b0 machine join</code></div>'
      html +=
        '</div><div class="modal-footer"><button class="btn btn-primary" onclick="this.closest(\'.modal-overlay\').remove()">Done</button></div></div></div>'
      document.body.insertAdjacentHTML('beforeend', html)
    },

    remove(id) {
      if (!confirm(`Remove machine "${id}"?`)) return
      App.api
        .del(`/machines/${encodeURIComponent(id)}`)
        .then(() => {
          App.toast.success('Machine removed')
          App.machines.render()
        })
        .catch((error) => {
          App.toast.error(`Failed: ${error.message}`)
        })
    },
  }

  App.machineDetail = {
    render(machineId) {
      if (!App.currentWorkspace) {
        setContent('<div class="empty-state"><p>No workspace selected.</p></div>')
        return
      }
      App.poll.stopAll()
      showLoading()
      Promise.all([App.api.get('/machines'), App.api.get(App.workspacePath('/agents'))])
        .then(([machinesData, agentsData]) => {
          const machines = machinesData.machines || []
          const agents = agentsData.agents || []
          const machine = machines.find((item) => item.id === machineId)
          const machineAgents = agents.filter((item) => item.machine_id === machineId)
          let html = ''
          html += '<div style="margin-bottom:16px"><a href="#/machines" style="color:var(--text-secondary);text-decoration:none;font-size:13px">&larr; Machines</a></div>'
          html += `<div class="page-header"><h2>${esc(machineId)}</h2>`
          if (machine) html += `<span style="font-size:14px">${statusDot(machine.status)}</span>`
          html +=
            `<button class="btn btn-primary" style="margin-left:auto" onclick="App.quickTask.showForMachine('${escAttr(machineId)}')">+ Quick Task</button></div>`
          if (machine && machine.last_heartbeat) {
            html += `<p style="color:var(--text-secondary);font-size:13px;margin-bottom:20px">Last seen ${timeAgo(machine.last_heartbeat)}</p>`
          }
          html +=
            `<div class="card"><div class="card-header">Agents<span style="font-weight:normal;color:var(--text-secondary);margin-left:8px">${machineAgents.length}</span></div>`
          html += '<table><thead><tr><th>Name</th><th>Status</th><th>Runtime</th><th>Description</th><th></th></tr></thead><tbody>'
          if (machineAgents.length === 0) {
            html +=
              '<tr><td colspan="5" style="text-align:center;color:var(--text-secondary);padding:32px">No agents on this machine</td></tr>'
          }
          machineAgents.forEach((agent) => {
            html += `<tr class="clickable" onclick="App.router.navigate('#/agents/${encodeURIComponent(agent.name)}')">`
            html += `<td><strong>${esc(agent.name)}</strong></td>`
            html += `<td>${statusDot(agent.status)}</td>`
            html += `<td>${esc(agent.runtime)}</td>`
            html += `<td style="color:var(--text-secondary)">${esc(truncate(agent.description, 50))}</td>`
            html +=
              `<td>${agent.status === 'active' ? `<button class="btn btn-sm btn-outline" onclick="event.stopPropagation(); App.detail.stop('${escAttr(agent.name)}')">Stop</button>` : `<button class="btn btn-sm btn-primary" onclick="event.stopPropagation(); App.detail.start('${escAttr(agent.name)}')">Start</button>`}</td>`
            html += '</tr>'
          })
          html += '</tbody></table></div>'
          setContent(html)
        })
        .catch((error) => {
          App.toast.error(`Failed to load machine: ${error.message}`)
        })
    },
  }

  App.agentsPage = {
    render() {
      if (!App.currentWorkspace) {
        setContent('<div class="empty-state"><p>No workspace selected.</p></div>')
        return
      }
      App.poll.stopAll()
      showLoading()
      App.api
        .get(App.workspacePath('/agents'))
        .then((data) => {
          const agents = data.agents || []
          const threadPromises = agents.map((agent) =>
            App.api
              .get(App.workspacePath(`/agents/${encodeURIComponent(agent.name)}/threads`))
              .then((threadData) => {
                const threads = threadData.threads || []
                let lastActive = null
                threads.forEach((thread) => {
                  if (!lastActive || thread.latest_at > lastActive) lastActive = thread.latest_at
                })
                return { name: agent.name, threadCount: threads.length, lastActive }
              })
              .catch(() => ({ name: agent.name, threadCount: 0, lastActive: null })),
          )
          return Promise.all(threadPromises).then((threadResults) => {
            const threadInfo = {}
            threadResults.forEach((thread) => {
              threadInfo[thread.name] = thread
            })
            let html = ''
            html += '<div class="page-header"><h2>Agents</h2>'
            html += '<button class="btn btn-primary" onclick="App.agentsPage.showAdd()">+ Add Agent</button></div>'
            html += '<div class="card"><table>'
            html +=
              '<thead><tr><th>Name</th><th>Machine</th><th>Status</th><th>Runtime</th><th>Instructions</th><th>Conversations</th><th>Created</th><th>Last Active</th><th></th></tr></thead><tbody>'
            if (agents.length === 0) {
              html += '<tr><td colspan="9" style="text-align:center;color:var(--text-secondary);padding:32px">No agents yet</td></tr>'
            }
            agents.forEach((agent) => {
              const info = threadInfo[agent.name] || {}
              html += `<tr class="clickable" onclick="App.router.navigate('#/agents/${encodeURIComponent(agent.name)}')">`
              html += `<td><strong>${esc(agent.name)}</strong></td>`
              html +=
                `<td><a href="#/machines/${encodeURIComponent(agent.machine_id)}" onclick="event.stopPropagation()" style="color:var(--primary);text-decoration:none">${esc(agent.machine_id)}</a></td>`
              html += `<td>${statusDot(agent.status)}</td>`
              html += `<td>${esc(agent.runtime)}</td>`
              html +=
                `<td style="color:var(--text-secondary);max-width:200px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${esc(truncate(agent.instructions, 50))}</td>`
              html += `<td>${info.threadCount || 0}</td>`
              html += `<td>${timeAgo(agent.created_at)}</td>`
              html += `<td>${info.lastActive ? timeAgo(info.lastActive) : '<span style="color:var(--text-secondary)">never</span>'}</td>`
              html +=
                `<td><button class="btn btn-sm btn-danger" onclick="event.stopPropagation(); App.agentsPage.remove('${escAttr(agent.name)}')">Remove</button></td>`
              html += '</tr>'
            })
            html += '</tbody></table></div>'
            setContent(html)
          })
        })
        .catch((error) => {
          App.toast.error(`Failed to load agents: ${error.message}`)
        })
    },

    showAdd() {
      App.api
        .get('/machines')
        .then((data) => {
          const machines = data.machines || []
          let html = '<div class="modal-overlay" onclick="if(event.target===this)this.remove()"><div class="modal">'
          html +=
            '<div class="modal-header">Add Agent<button class="btn-icon" onclick="this.closest(\'.modal-overlay\').remove()">&times;</button></div>'
          html += '<div class="modal-body">'
          html += '<div class="form-group"><label>Name</label><input id="add-a-name" placeholder="e.g. reviewer"></div>'
          html += '<div class="form-group"><label>Description</label><input id="add-a-desc" placeholder="Optional description"></div>'
          html += '<div class="form-group"><label>Instructions</label><textarea id="add-a-instructions" placeholder="What should this agent do?"></textarea></div>'
          html += '<div class="form-row"><div class="form-group"><label>Machine</label><select id="add-a-machine">'
          machines.forEach((machine) => {
            html += `<option value="${escAttr(machine.id)}">${esc(machine.id)}</option>`
          })
          if (machines.length === 0) html += '<option value="local">local</option>'
          html += '</select></div><div class="form-group"><label>Runtime</label><select id="add-a-runtime">'
          html += '<option value="auto">auto</option><option value="claude">claude</option><option value="codex">codex</option>'
          html += '</select></div></div></div>'
          html += '<div class="modal-footer"><button class="btn btn-outline" onclick="this.closest(\'.modal-overlay\').remove()">Cancel</button>'
          html += '<button class="btn btn-primary" onclick="App.agentsPage.add()">Add Agent</button></div></div></div>'
          document.body.insertAdjacentHTML('beforeend', html)
          document.getElementById('add-a-name').focus()
        })
        .catch((error) => {
          App.toast.error(`Failed: ${error.message}`)
        })
    },

    add() {
      const name = document.getElementById('add-a-name').value.trim()
      const desc = document.getElementById('add-a-desc').value.trim()
      const instructions = document.getElementById('add-a-instructions').value.trim()
      const machine = document.getElementById('add-a-machine').value
      const runtime = document.getElementById('add-a-runtime').value
      if (!name || !instructions) {
        App.toast.error('Name and instructions are required')
        return
      }
      App.api
        .post(App.workspacePath('/agents'), {
          name,
          description: desc,
          instructions,
          machine_id: machine,
          runtime,
        })
        .then(() => {
          document.querySelector('.modal-overlay').remove()
          App.toast.success(`Agent "${name}" added`)
          App.agentsPage.render()
        })
        .catch((error) => {
          App.toast.error(`Failed: ${error.message}`)
        })
    },

    remove(name) {
      if (!confirm(`Remove agent "${name}"?`)) return
      App.api
        .del(App.workspacePath(`/agents/${encodeURIComponent(name)}`))
        .then(() => {
          App.toast.success('Agent removed')
          App.agentsPage.render()
        })
        .catch((error) => {
          App.toast.error(`Failed: ${error.message}`)
        })
    },
  }

  App.detail = {
    _expandedThread: null,

    render(name, expandThreadId) {
      if (!App.currentWorkspace) return
      App.poll.stopAll()
      App.detail._expandedThread = expandThreadId || null
      showLoading()

      Promise.all([
        App.api.get(App.workspacePath(`/agents/${encodeURIComponent(name)}`)),
        App.api.get(App.workspacePath(`/agents/${encodeURIComponent(name)}/threads`)),
      ])
        .then(([worker, threadsData]) => {
          const threads = threadsData.threads || []

          let html = ''
          html += '<div style="margin-bottom:16px">'
          html += '<a href="#/agents" style="color:var(--text-secondary);text-decoration:none;font-size:13px">&larr; Agents</a>'
          html += '</div>'
          html += '<div class="page-header">'
          html += `<h2>${esc(worker.name)}</h2>`
          html += '<div>'
          if (worker.status === 'active') {
            html += `<button class="btn btn-sm btn-outline" onclick="App.detail.stop('${escAttr(worker.name)}')">Stop</button> `
          } else {
            html += `<button class="btn btn-sm btn-primary" onclick="App.detail.start('${escAttr(worker.name)}')">Start</button> `
          }
          html += `<button class="btn btn-sm btn-danger" onclick="App.detail.remove('${escAttr(worker.name)}')">Remove</button>`
          html += '</div></div>'

          html += '<div class="card" style="margin-bottom:20px">'
          html += '<div class="card-header">Details</div>'
          html += '<div class="card-body">'
          html += '<dl class="detail-grid">'
          html += `<dt>Name</dt><dd>${esc(worker.name)}</dd>`
          html += `<dt>Description</dt><dd>${esc(worker.description || '(none)')}</dd>`
          html += `<dt>Machine</dt><dd>${esc(worker.machine_id)}</dd>`
          html += `<dt>Runtime</dt><dd>${esc(worker.runtime)}</dd>`
          html += `<dt>Status</dt><dd>${statusDot(worker.status)}</dd>`
          if (worker.instructions) {
            html += `<dt>Instructions</dt><dd><div class="instructions-block">${esc(worker.instructions)}</div></dd>`
          }
          html += '</dl></div></div>'

          html += '<div class="card" id="conversations-card">'
          html += '<div class="card-header">Conversations '
          html += `<button class="btn btn-sm btn-primary" onclick="App.detail.showNewConvo('${escAttr(worker.name)}')">+ New Conversation</button>`
          html += '</div>'

          if (threads.length === 0) {
            html += '<div class="card-body"><p style="color:var(--text-secondary)">No conversations yet.</p></div>'
          } else {
            html += '<div id="thread-list">'
            threads.forEach((thread) => {
              const title = contentAsText(thread.first_content)
              html +=
                `<div class="thread-row" data-thread="${escAttr(thread.thread_id)}" onclick="App.detail.toggleThread('${escAttr(thread.thread_id)}', '${escAttr(worker.name)}')">`
              html += `<span class="tr-id">${esc(truncate(thread.thread_id, 14))}</span>`
              html += `<span class="tr-title">${esc(truncate(title, 60))}</span>`
              html += `<span class="thread-msg-type ${esc(thread.latest_type)}">${esc(thread.latest_type)}</span>`
              html += `<span class="tr-time">${timeAgo(thread.latest_at)}</span>`
              html += '</div>'
              html += `<div id="convo-${escAttr(thread.thread_id)}" style="display:none"></div>`
            })
            html += '</div>'
          }
          html += '</div>'

          setContent(html)

          if (expandThreadId) {
            App.detail.toggleThread(expandThreadId, worker.name)
          }
        })
        .catch((error) => {
          App.toast.error(`Failed to load agent: ${error.message}`)
        })
    },

    toggleThread(threadId, workerName) {
      const container = document.getElementById(`convo-${threadId}`)
      if (!container) return

      if (container.style.display !== 'none') {
        container.style.display = 'none'
        container.innerHTML = ''
        App.poll.stop(threadId)
        App.detail._expandedThread = null
        return
      }

      if (App.detail._expandedThread && App.detail._expandedThread !== threadId) {
        const prev = document.getElementById(`convo-${App.detail._expandedThread}`)
        if (prev) {
          prev.style.display = 'none'
          prev.innerHTML = ''
        }
        App.poll.stop(App.detail._expandedThread)
      }

      App.detail._expandedThread = threadId
      container.style.display = 'block'
      container.innerHTML = '<div class="convo-area"><div class="loading"><span class="spinner"></span></div></div>'

      App.api
        .get(App.workspacePath(`/threads/${encodeURIComponent(threadId)}`))
        .then((data) => {
          const msgs = data.messages || []
          App.detail._renderConvo(container, msgs, threadId, workerName)

          if (msgs.length > 0) {
            const lastType = msgs[msgs.length - 1].msg_type || msgs[msgs.length - 1].type
            if (lastType === 'request' || lastType === 'answer') {
              App.poll.start(threadId, workerName, (newMsgs) => {
                App.detail._renderConvo(container, newMsgs, threadId, workerName)
              })
            }
          }
        })
        .catch((error) => {
          container.innerHTML = `<div class="convo-area"><p style="color:var(--error)">Failed to load: ${esc(error.message)}</p></div>`
        })
    },

    _renderConvo(container, msgs, threadId, workerName) {
      let html = '<div class="convo-area">'

      if (msgs.length === 0) {
        html += '<p style="color:var(--text-secondary)">No messages.</p>'
      } else {
        html += '<div class="thread-messages">'
        msgs.forEach((message) => {
          html += '<div class="thread-msg">'
          html += '<div class="thread-msg-header">'
          html += `<strong>${esc(message.from_id || message.from)}</strong>`
          html += ` &rarr; ${esc(message.to_id || message.to)}`
          html += ` <span class="thread-msg-type ${esc(message.msg_type || message.type)}">${esc(message.msg_type || message.type)}</span>`
          html += ` <span style="margin-left:auto">${timeAgo(message.created_at)}</span>`
          html += '</div>'
          const text = contentAsText(message.content)
          if (text) {
            html += `<div class="thread-msg-content">${esc(text)}</div>`
          }
          html += '</div>'
        })
        html += '</div>'

        const lastType = msgs[msgs.length - 1].msg_type || msgs[msgs.length - 1].type
        if (lastType === 'request' || lastType === 'answer') {
          html += `<div class="poll-indicator"><span class="spinner"></span> ${esc(workerName)} is working...</div>`
        }

        if (lastType === 'question') {
          html += '<div class="reply-row">'
          html += `<input id="reply-input-${escAttr(threadId)}" placeholder="Type your reply..." onkeydown="if(event.key==='Enter')App.detail.sendReply('${escAttr(threadId)}','${escAttr(workerName)}')">`
          html += `<button class="btn btn-sm btn-primary" onclick="App.detail.sendReply('${escAttr(threadId)}','${escAttr(workerName)}')">Send</button>`
          html += '</div>'
        }
      }

      html += '</div>'
      container.innerHTML = html

      const msgsEl = container.querySelector('.thread-messages')
      if (msgsEl) msgsEl.scrollTop = msgsEl.scrollHeight
    },

    sendReply(threadId, workerName) {
      const input = document.getElementById(`reply-input-${threadId}`)
      if (!input) return
      const text = input.value.trim()
      if (!text) return
      input.disabled = true

      App.webAgent
        .ensureRegistered()
        .then((agentId) =>
          App.api.post(App.workspacePath(`/agents/${encodeURIComponent(workerName)}/inbox`), {
            thread_id: threadId,
            from: agentId,
            type: 'answer',
            content: text,
          }),
        )
        .then(() => {
          const container = document.getElementById(`convo-${threadId}`)
          App.poll.start(threadId, workerName, (msgs) => {
            App.detail._renderConvo(container, msgs, threadId, workerName)
          })
        })
        .catch((error) => {
          App.toast.error(`Failed to send: ${error.message}`)
          input.disabled = false
        })
    },

    showNewConvo(workerName) {
      let html = '<div class="modal-overlay" onclick="if(event.target===this)this.remove()">'
      html += '<div class="modal">'
      html += `<div class="modal-header">New Conversation with ${esc(workerName)}<button class="btn-icon" onclick="this.closest('.modal-overlay').remove()">&times;</button></div>`
      html += '<div class="modal-body">'
      html += '<div class="form-group"><label>Task / Message</label><textarea id="new-convo-content" placeholder="What would you like this worker to do?"></textarea></div>'
      html += '</div>'
      html += '<div class="modal-footer">'
      html += '<button class="btn btn-outline" onclick="this.closest(\'.modal-overlay\').remove()">Cancel</button>'
      html += `<button class="btn btn-primary" onclick="App.detail.createConvo('${escAttr(workerName)}')">Send</button>`
      html += '</div></div></div>'
      document.body.insertAdjacentHTML('beforeend', html)
      document.getElementById('new-convo-content').focus()
    },

    createConvo(workerName) {
      const content = document.getElementById('new-convo-content').value.trim()
      if (!content) {
        App.toast.error('Message is required')
        return
      }

      App.webAgent
        .ensureRegistered()
        .then((agentId) => {
          const threadId = `thread-${crypto.randomUUID().slice(0, 8)}`
          return App.api
            .post(App.workspacePath(`/agents/${encodeURIComponent(workerName)}/inbox`), {
              thread_id: threadId,
              from: agentId,
              type: 'request',
              content,
            })
            .then(() => threadId)
        })
        .then((threadId) => {
          document.querySelector('.modal-overlay').remove()
          App.toast.success('Conversation started')
          App.router.navigate(`#/agents/${encodeURIComponent(workerName)}/${encodeURIComponent(threadId)}`)
        })
        .catch((error) => {
          App.toast.error(`Failed: ${error.message}`)
        })
    },

    stop(name) {
      App.api
        .post(App.workspacePath(`/agents/${encodeURIComponent(name)}/stop`))
        .then(() => {
          App.toast.success('Agent stopped')
          App.detail.render(name)
        })
        .catch((error) => {
          App.toast.error(`Failed: ${error.message}`)
        })
    },

    start(name) {
      App.api
        .post(App.workspacePath(`/agents/${encodeURIComponent(name)}/start`))
        .then(() => {
          App.toast.success('Agent started')
          App.detail.render(name)
        })
        .catch((error) => {
          App.toast.error(`Failed: ${error.message}`)
        })
    },

    remove(name) {
      if (!confirm(`Remove agent "${name}"?`)) return
      App.api
        .del(App.workspacePath(`/agents/${encodeURIComponent(name)}`))
        .then(() => {
          App.toast.success('Agent removed')
          App.router.navigate('#/agents')
        })
        .catch((error) => {
          App.toast.error(`Failed: ${error.message}`)
        })
    },
  }

  App.quickTask = {
    show() {
      App.quickTask.showForMachine(null)
    },

    showForMachine(preselectedMachine) {
      App.api
        .get('/machines')
        .then((data) => {
          const machines = data.machines || []
          let html = '<div class="modal-overlay" onclick="if(event.target===this)this.remove()">'
          html += '<div class="modal">'
          html += '<div class="modal-header">Quick Task<button class="btn-icon" onclick="this.closest(\'.modal-overlay\').remove()">&times;</button></div>'
          html += '<div class="modal-body">'
          html += '<div class="form-group"><label>Instructions</label><textarea id="qt-instructions" placeholder="e.g. Review code carefully, focus on security..."></textarea></div>'
          html += '<div class="form-group"><label>Task</label><textarea id="qt-task" placeholder="What should this agent do?"></textarea></div>'
          html += '<div class="form-row">'
          html += '<div class="form-group"><label>Machine</label><select id="qt-machine">'
          machines.forEach((machine) => {
            const selected = preselectedMachine && machine.id === preselectedMachine ? ' selected' : ''
            html += `<option value="${escAttr(machine.id)}"${selected}>${esc(machine.id)}</option>`
          })
          if (machines.length === 0) html += '<option value="local">local</option>'
          html += '</select></div>'
          html += '<div class="form-group"><label>Runtime</label><select id="qt-runtime">'
          html += '<option value="auto">auto</option>'
          html += '<option value="claude">claude</option>'
          html += '<option value="codex">codex</option>'
          html += '</select></div>'
          html += '</div>'
          html += '</div>'
          html += '<div class="modal-footer">'
          html += '<button class="btn btn-outline" onclick="this.closest(\'.modal-overlay\').remove()">Cancel</button>'
          html += '<button class="btn btn-primary" onclick="App.quickTask.run()">Run</button>'
          html += '</div></div></div>'
          document.body.insertAdjacentHTML('beforeend', html)
          document.getElementById('qt-instructions').focus()
        })
        .catch((error) => {
          App.toast.error(`Failed to load machines: ${error.message}`)
        })
    },

    run() {
      const instructions = document.getElementById('qt-instructions').value.trim()
      const task = document.getElementById('qt-task').value.trim()
      const machine = document.getElementById('qt-machine').value
      const runtime = document.getElementById('qt-runtime').value

      if (!instructions || !task) {
        App.toast.error('Instructions and task are required')
        return
      }

      const agentName = `task-${crypto.randomUUID().slice(0, 8)}`

      App.api
        .post(App.workspacePath('/agents'), {
          name: agentName,
          description: 'Quick task',
          instructions,
          machine_id: machine,
          runtime,
        })
        .then(() =>
          App.webAgent.ensureRegistered().then((webAgentId) => {
            const threadId = `thread-${crypto.randomUUID().slice(0, 8)}`
            return App.api
              .post(App.workspacePath(`/agents/${encodeURIComponent(agentName)}/inbox`), {
                thread_id: threadId,
                from: webAgentId,
                type: 'request',
                content: task,
              })
              .then(() => ({ agentName, threadId }))
          }),
        )
        .then((result) => {
          document.querySelector('.modal-overlay').remove()
          App.toast.success('Quick task started')
          App.router.navigate(`#/agents/${encodeURIComponent(result.agentName)}/${encodeURIComponent(result.threadId)}`)
        })
        .catch((error) => {
          App.toast.error(`Failed: ${error.message}`)
        })
    },
  }

  App.workspacesPage = {
    render() {
      App.poll.stopAll()
      showLoading()
      App.api
        .get('/workspaces')
        .then((data) => {
          const workspaces = data.workspaces || []
          let html = ''
          html += '<div class="page-header"><h2>Workspaces</h2>'
          html += '<button class="btn btn-primary" onclick="App.workspacesPage.showCreate()">+ Create Workspace</button>'
          html += '</div>'

          html += '<div class="card">'
          if (workspaces.length === 0) {
            html += '<div class="empty-state"><p>No workspaces yet.</p></div>'
          } else {
            html += '<table>'
            html += '<thead><tr><th>Name</th><th>Created By</th><th>Created</th><th></th></tr></thead>'
            html += '<tbody>'
            workspaces.forEach((workspace) => {
              html += '<tr>'
              html += `<td><strong>${esc(workspace.name)}</strong></td>`
              html += `<td>${esc(workspace.created_by)}</td>`
              html += `<td>${timeAgo(workspace.created_at)}</td>`
              html += `<td><button class="btn btn-sm btn-outline" onclick="App.workspacesPage.showAddMember('${escAttr(workspace.name)}')">Add Member</button></td>`
              html += '</tr>'
            })
            html += '</tbody></table>'
          }
          html += '</div>'

          setContent(html)
        })
        .catch((error) => {
          App.toast.error(`Failed to load workspaces: ${error.message}`)
        })
    },

    showCreate() {
      let html = '<div class="modal-overlay" onclick="if(event.target===this)this.remove()">'
      html += '<div class="modal">'
      html += '<div class="modal-header">Create Workspace<button class="btn-icon" onclick="this.closest(\'.modal-overlay\').remove()">&times;</button></div>'
      html += '<div class="modal-body">'
      html += '<div class="form-group"><label>Workspace Name</label><input id="create-workspace-name" placeholder="e.g. my-team"></div>'
      html += '</div>'
      html += '<div class="modal-footer">'
      html += '<button class="btn btn-outline" onclick="this.closest(\'.modal-overlay\').remove()">Cancel</button>'
      html += '<button class="btn btn-primary" onclick="App.workspacesPage.create()">Create</button>'
      html += '</div></div></div>'
      document.body.insertAdjacentHTML('beforeend', html)
      document.getElementById('create-workspace-name').focus()
    },

    create() {
      const name = document.getElementById('create-workspace-name').value.trim()
      if (!name) {
        App.toast.error('Name is required')
        return
      }
      App.api
        .post('/workspaces', { name })
        .then(() => {
          document.querySelector('.modal-overlay').remove()
          App.toast.success(`Workspace "${name}" created`)
          App.api.get('/workspaces').then((data) => {
            App.workspaces = data.workspaces || []
            const sel = document.getElementById('workspace-select')
            sel.innerHTML = ''
            App.workspaces.forEach((workspace) => {
              const opt = document.createElement('option')
              opt.value = workspace.name
              opt.textContent = workspace.name
              sel.appendChild(opt)
            })
            if (App.currentWorkspace) sel.value = App.currentWorkspace
          })
          App.workspacesPage.render()
        })
        .catch((error) => {
          App.toast.error(`Failed: ${error.message}`)
        })
    },

    showAddMember(workspaceName) {
      let html = '<div class="modal-overlay" onclick="if(event.target===this)this.remove()">'
      html += '<div class="modal">'
      html += `<div class="modal-header">Add Member to ${esc(workspaceName)}<button class="btn-icon" onclick="this.closest('.modal-overlay').remove()">&times;</button></div>`
      html += '<div class="modal-body">'
      html += '<div class="form-group"><label>User ID</label><input id="add-member-id" placeholder="User ID"></div>'
      html += '</div>'
      html += '<div class="modal-footer">'
      html += '<button class="btn btn-outline" onclick="this.closest(\'.modal-overlay\').remove()">Cancel</button>'
      html += `<button class="btn btn-primary" onclick="App.workspacesPage.addMember('${escAttr(workspaceName)}')">Add</button>`
      html += '</div></div></div>'
      document.body.insertAdjacentHTML('beforeend', html)
      document.getElementById('add-member-id').focus()
    },

    addMember(workspaceName) {
      const userId = document.getElementById('add-member-id').value.trim()
      if (!userId) {
        App.toast.error('User ID is required')
        return
      }
      App.api
        .post(`/workspaces/${encodeURIComponent(workspaceName)}/members/${encodeURIComponent(userId)}`)
        .then(() => {
          document.querySelector('.modal-overlay').remove()
          App.toast.success('Member added')
          App.workspacesPage.render()
        })
        .catch((error) => {
          App.toast.error(`Failed: ${error.message}`)
        })
    },
  }

  App.usersPage = {
    render() {
      App.poll.stopAll()
      showLoading()
      App.api
        .get('/users')
        .then((data) => {
          const users = data.users || []
          let html = ''
          html += '<div class="page-header"><h2>Users</h2>'
          html += '<button class="btn btn-primary" onclick="App.usersPage.showInvite()">+ Invite User</button>'
          html += '</div>'

          html += '<div class="card">'
          if (users.length === 0) {
            html += '<div class="empty-state"><p>No users.</p></div>'
          } else {
            html += '<table>'
            html += '<thead><tr><th>ID</th><th>Name</th><th>Admin</th><th>Created</th></tr></thead>'
            html += '<tbody>'
            users.forEach((user) => {
              html += '<tr>'
              html += `<td style="font-family:var(--mono);font-size:12px">${esc(truncate(user.id, 16))}</td>`
              html += `<td>${esc(user.name)}</td>`
              html += `<td>${user.is_admin ? 'Yes' : 'No'}</td>`
              html += `<td>${timeAgo(user.created_at)}</td>`
              html += '</tr>'
            })
            html += '</tbody></table>'
          }
          html += '</div>'

          setContent(html)
        })
        .catch(() => {
          setContent('<div class="card"><div class="empty-state"><p>User management is only available to admins.</p></div></div>')
        })
    },

    showInvite() {
      let html = '<div class="modal-overlay" onclick="if(event.target===this)this.remove()">'
      html += '<div class="modal">'
      html += '<div class="modal-header">Invite User<button class="btn-icon" onclick="this.closest(\'.modal-overlay\').remove()">&times;</button></div>'
      html += '<div class="modal-body">'
      html += '<div class="form-group"><label>Name</label><input id="invite-name" placeholder="e.g. alice"></div>'
      html += '</div>'
      html += '<div class="modal-footer">'
      html += '<button class="btn btn-outline" onclick="this.closest(\'.modal-overlay\').remove()">Cancel</button>'
      html += '<button class="btn btn-primary" onclick="App.usersPage.invite()">Invite</button>'
      html += '</div></div></div>'
      document.body.insertAdjacentHTML('beforeend', html)
      document.getElementById('invite-name').focus()
    },

    invite() {
      const name = document.getElementById('invite-name').value.trim()
      if (!name) {
        App.toast.error('Name is required')
        return
      }
      App.api
        .post('/users/invite', { name })
        .then((data) => {
          document.querySelector('.modal-overlay').remove()
          let html = '<div class="modal-overlay" onclick="if(event.target===this)this.remove()">'
          html += '<div class="modal">'
          html += '<div class="modal-header">User Invited<button class="btn-icon" onclick="this.closest(\'.modal-overlay\').remove()">&times;</button></div>'
          html += '<div class="modal-body">'
          html += `<p style="margin-bottom:12px">Share this API key with <strong>${esc(data.name)}</strong>. It will not be shown again.</p>`
          html += `<div class="form-group"><label>API Key</label><input type="text" value="${escAttr(data.key)}" readonly onclick="this.select()" style="font-family:var(--mono)"></div>`
          html += `<div class="form-group"><label>User ID</label><input type="text" value="${escAttr(data.user_id)}" readonly onclick="this.select()" style="font-family:var(--mono)"></div>`
          html += '</div>'
          html += '<div class="modal-footer">'
          html += '<button class="btn btn-primary" onclick="this.closest(\'.modal-overlay\').remove()">Done</button>'
          html += '</div></div></div>'
          document.body.insertAdjacentHTML('beforeend', html)
          App.usersPage.render()
        })
        .catch((error) => {
          App.toast.error(`Failed: ${error.message}`)
        })
    },
  }

  document.getElementById('login-key').addEventListener('keydown', (event) => {
    if (event.key === 'Enter') App.auth.login()
  })

  const params = new URLSearchParams(window.location.search)
  const urlKey = params.get('key')
  if (urlKey) {
    window.history.replaceState({}, '', window.location.pathname)
    App.api.key = urlKey
    App.api
      .get('/workspaces')
      .then((data) => {
        localStorage.setItem('b0_api_key', urlKey)
        App.boot(data)
      })
      .catch(() => {
        App.api.key = null
        App.auth.tryRestore()
      })
  } else {
    App.auth.tryRestore()
  }

  return () => {
    App.poll.stopAll()
    if (App.tasksPage._boardTimer) clearInterval(App.tasksPage._boardTimer)
    if (App.tasksPage._chatTimer) clearInterval(App.tasksPage._chatTimer)
    window.removeEventListener('hashchange', App.router.onHashChange)
    document.querySelectorAll('.modal-overlay').forEach((modal) => modal.remove())
    delete window.App
    root.innerHTML = ''
  }
}
