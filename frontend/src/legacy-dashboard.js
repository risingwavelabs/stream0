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
      <a href="#/workflows" data-page="workflows"><span class="nav-icon">W</span> Workflows</a>
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
        return res.text().then((text) => {
          const contentType = res.headers.get('content-type') || ''
          const isJson = contentType.includes('application/json')
          const data = text
            ? isJson
              ? JSON.parse(text)
              : null
            : null

          if (!res.ok) {
            const message =
              (data && data.error) ||
              text ||
              `Request failed (${res.status})`
            throw new Error(message)
          }

          if (data !== null) return data
          return {}
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
      '/workflows': () => {
        App.workflowsPage.render()
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
      if (App.workflowDetail && App.workflowDetail._runTimer) {
        clearInterval(App.workflowDetail._runTimer)
        App.workflowDetail._runTimer = null
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

      if (parts[0] === 'workflows' && parts[1]) {
        App.workflowDetail.render(decodeURIComponent(parts[1]))
        document.querySelectorAll('.sidebar-nav a').forEach((link) => {
          link.classList.toggle('active', link.getAttribute('data-page') === 'workflows')
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

  function defaultNodeTitle(kind) {
    if (kind === 'start') return 'Start'
    if (kind === 'agent') return 'Agent Step'
    if (kind === 'human_input') return 'Human Input'
    if (kind === 'end') return 'End'
    return 'Step'
  }

  function workflowPreviewLayout(definition) {
    const nodes = definition.nodes || []
    const edges = definition.edges || []
    const width = 190
    const height = 92
    const padding = 24
    const columnGap = 230
    const rowGap = 130

    const uniquePositions = new Set(
      nodes.map((node) => `${Math.round(node.position_x || 0)}:${Math.round(node.position_y || 0)}`),
    )
    const useSavedPositions = uniquePositions.size > 1

    if (useSavedPositions) {
      let minX = Infinity
      let minY = Infinity
      nodes.forEach((node) => {
        minX = Math.min(minX, node.position_x || 0)
        minY = Math.min(minY, node.position_y || 0)
      })

      const positions = {}
      let maxRight = 0
      let maxBottom = 0
      nodes.forEach((node) => {
        const x = padding + (node.position_x || 0) - minX
        const y = padding + (node.position_y || 0) - minY
        positions[node.id] = { x, y }
        maxRight = Math.max(maxRight, x + width)
        maxBottom = Math.max(maxBottom, y + height)
      })
      return {
        width: maxRight + padding,
        height: maxBottom + padding,
        nodeWidth: width,
        nodeHeight: height,
        positions,
      }
    }

    const nodeMap = {}
    nodes.forEach((node) => {
      nodeMap[node.id] = node
    })

    const incoming = {}
    const outgoing = {}
    nodes.forEach((node) => {
      incoming[node.id] = []
      outgoing[node.id] = []
    })
    edges.forEach((edge) => {
      if (incoming[edge.target_node_id]) incoming[edge.target_node_id].push(edge.source_node_id)
      if (outgoing[edge.source_node_id]) outgoing[edge.source_node_id].push(edge.target_node_id)
    })

    const depth = {}
    const indegree = {}
    nodes.forEach((node) => {
      indegree[node.id] = incoming[node.id].length
      depth[node.id] = node.kind === 'start' ? 0 : 1
    })

    const queue = nodes
      .filter((node) => indegree[node.id] === 0)
      .sort((a, b) => a.title.localeCompare(b.title))
      .map((node) => node.id)

    while (queue.length) {
      const nodeId = queue.shift()
      ;(outgoing[nodeId] || []).forEach((targetId) => {
        depth[targetId] = Math.max(depth[targetId] || 0, (depth[nodeId] || 0) + 1)
        indegree[targetId] -= 1
        if (indegree[targetId] === 0) queue.push(targetId)
      })
    }

    const columns = {}
    nodes.forEach((node) => {
      const col = depth[node.id] || 0
      if (!columns[col]) columns[col] = []
      columns[col].push(node)
    })

    Object.values(columns).forEach((column) => {
      column.sort((a, b) => {
        const kindOrder = { start: 0, agent: 1, human_input: 2, end: 3 }
        const diff = (kindOrder[a.kind] || 9) - (kindOrder[b.kind] || 9)
        if (diff !== 0) return diff
        return (a.title || '').localeCompare(b.title || '')
      })
    })

    const positions = {}
    let maxRight = 0
    let maxBottom = 0
    Object.entries(columns).forEach(([columnIndex, column]) => {
      column.forEach((node, rowIndex) => {
        const x = padding + Number(columnIndex) * columnGap
        const y = padding + rowIndex * rowGap
        positions[node.id] = { x, y }
        maxRight = Math.max(maxRight, x + width)
        maxBottom = Math.max(maxBottom, y + height)
      })
    })

    return {
      width: maxRight + padding,
      height: maxBottom + padding,
      nodeWidth: width,
      nodeHeight: height,
      positions,
    }
  }

  function workflowEdgePath(from, to, nodeWidth, nodeHeight) {
    const startX = from.x + nodeWidth
    const startY = from.y + nodeHeight / 2
    const endX = to.x
    const endY = to.y + nodeHeight / 2
    const curve = Math.max(40, Math.abs(endX - startX) * 0.35)
    return `M ${startX} ${startY} C ${startX + curve} ${startY}, ${endX - curve} ${endY}, ${endX} ${endY}`
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

  App.workflowsPage = {
    render() {
      if (!App.currentWorkspace) {
        setContent('<div class="empty-state"><p>No workspace selected.</p></div>')
        return
      }

      App.poll.stopAll()
      showLoading()

      Promise.all([
        App.api.get(App.workspacePath('/agents')),
        App.api.get(App.workspacePath('/workflows')),
      ])
        .then(([agentsData, workflowsData]) => {
          const agents = agentsData.agents || []
          const workflows = workflowsData.workflows || []
          let html = ''
          html += '<div class="page-header"><h2>Workflows</h2>'
          html += `<button class="btn btn-primary"${agents.length === 0 ? ' disabled' : ''} onclick="App.workflowsPage.showAdd()">+ Create Workflow</button></div>`

          if (agents.length === 0) {
            html += '<div class="card"><div class="empty-state">'
            html += '<p>Workflows need at least one agent in this workspace.</p>'
            html += '<a class="btn btn-primary" href="#/agents">Create an Agent First</a>'
            html += '</div></div>'
            setContent(html)
            return
          }

          if (workflows.length === 0) {
            html += '<div class="card"><div class="empty-state">'
            html += '<p>No workflows yet.</p>'
            html += '<button class="btn btn-primary" onclick="App.workflowsPage.showAdd()">Create Your First Workflow</button>'
            html += '</div></div>'
            setContent(html)
            return
          }

          html += '<div class="card"><table>'
          html += '<thead><tr><th>Name</th><th>Status</th><th>Nodes</th><th>Agents</th><th>Updated</th><th>Created By</th></tr></thead><tbody>'
          workflows.forEach((workflow) => {
            html += `<tr class="clickable" onclick="App.router.navigate('#/workflows/${encodeURIComponent(workflow.id)}')">`
            html += `<td><strong>${esc(workflow.name)}</strong><div style="color:var(--text-secondary);font-size:12px">${esc(truncate(workflow.description || '(no description)', 80))}</div></td>`
            html += `<td>${statusDot(workflow.status)}</td>`
            html += `<td>${esc(workflow.node_count)}</td>`
            html += `<td>${esc(workflow.agent_count)}</td>`
            html += `<td>${timeAgo(workflow.updated_at)}</td>`
            html += `<td>${esc(workflow.created_by)}</td>`
            html += '</tr>'
          })
          html += '</tbody></table></div>'
          setContent(html)
        })
        .catch((error) => {
          App.toast.error(`Failed to load workflows: ${error.message}`)
        })
    },

    showAdd() {
      App.api
        .get(App.workspacePath('/agents'))
        .then((data) => {
          const agents = data.agents || []
          if (agents.length === 0) {
            App.toast.error('Create an agent before creating a workflow')
            return
          }

          let html = '<div class="modal-overlay" onclick="if(event.target===this)this.remove()"><div class="modal">'
          html += '<div class="modal-header">Create Workflow<button class="btn-icon" onclick="this.closest(\'.modal-overlay\').remove()">&times;</button></div>'
          html += '<div class="modal-body">'
          html += '<div class="form-group"><label>Name</label><input id="wf-create-name" placeholder="e.g. Research and Review"></div>'
          html += '<div class="form-group"><label>Description</label><textarea id="wf-create-description" placeholder="What is this workflow for?"></textarea></div>'
          html += '<div class="form-group"><label>First Agent Step</label><select id="wf-create-agent">'
          agents.forEach((agent) => {
            html += `<option value="${escAttr(agent.name)}">${esc(agent.name)}</option>`
          })
          html += '</select></div>'
          html += '</div>'
          html += '<div class="modal-footer"><button class="btn btn-outline" onclick="this.closest(\'.modal-overlay\').remove()">Cancel</button>'
          html += '<button class="btn btn-primary" onclick="App.workflowsPage.create()">Create</button></div></div></div>'
          document.body.insertAdjacentHTML('beforeend', html)
          document.getElementById('wf-create-name').focus()
        })
        .catch((error) => {
          App.toast.error(`Failed: ${error.message}`)
        })
    },

    create() {
      const name = document.getElementById('wf-create-name').value.trim()
      const description = document.getElementById('wf-create-description').value.trim()
      const firstAgent = document.getElementById('wf-create-agent').value
      if (!name || !firstAgent) {
        App.toast.error('Name and first agent are required')
        return
      }

      const startId = `node-${crypto.randomUUID().slice(0, 8)}`
      const agentId = `node-${crypto.randomUUID().slice(0, 8)}`
      const endId = `node-${crypto.randomUUID().slice(0, 8)}`
      const payload = {
        name,
        description,
        status: 'draft',
        nodes: [
          { id: startId, kind: 'start', title: 'Start', prompt: '', position_x: 0, position_y: 0 },
          { id: agentId, kind: 'agent', title: 'Agent Step', prompt: '', agent_name: firstAgent, position_x: 220, position_y: 0 },
          { id: endId, kind: 'end', title: 'End', prompt: '', position_x: 440, position_y: 0 },
        ],
        edges: [
          { id: `edge-${crypto.randomUUID().slice(0, 8)}`, source_node_id: startId, target_node_id: agentId },
          { id: `edge-${crypto.randomUUID().slice(0, 8)}`, source_node_id: agentId, target_node_id: endId },
        ],
      }

      App.api
        .post(App.workspacePath('/workflows'), payload)
        .then((workflow) => {
          document.querySelector('.modal-overlay').remove()
          App.toast.success('Workflow created')
          App.router.navigate(`#/workflows/${encodeURIComponent(workflow.id)}`)
        })
        .catch((error) => {
          App.toast.error(`Failed: ${error.message}`)
        })
    },
  }

  App.workflowDetail = {
    _definition: null,
    _agents: [],
    _runs: [],
    _selectedRunId: null,
    _selectedRunDetail: null,
    _workflowId: null,
    _runTimer: null,
    _threadMessages: {},
    _expandedThreadId: null,

    render(workflowId) {
      if (!App.currentWorkspace) return
      App.poll.stopAll()
      App.workflowDetail._workflowId = workflowId
      App.workflowDetail._selectedRunId = null
      App.workflowDetail._selectedRunDetail = null
      App.workflowDetail._threadMessages = {}
      App.workflowDetail._expandedThreadId = null
      if (App.workflowDetail._runTimer) {
        clearInterval(App.workflowDetail._runTimer)
        App.workflowDetail._runTimer = null
      }
      App.workflowDetail.load(true)
    },

    load(showSpinner = false) {
      if (!App.currentWorkspace || !App.workflowDetail._workflowId) return
      if (showSpinner) showLoading()

      Promise.all([
        App.api.get(App.workspacePath(`/workflows/${encodeURIComponent(App.workflowDetail._workflowId)}`)),
        App.api.get(App.workspacePath('/agents')),
        App.api.get(App.workspacePath(`/workflow-runs?workflow_id=${encodeURIComponent(App.workflowDetail._workflowId)}`)),
      ])
        .then(([definition, agentsData, runsData]) => {
          App.workflowDetail._definition = definition
          App.workflowDetail._agents = agentsData.agents || []
          App.workflowDetail._runs = runsData.runs || []

          const selectedRunId =
            App.workflowDetail._selectedRunId ||
            (App.workflowDetail._runs[0] ? App.workflowDetail._runs[0].id : null)

          if (selectedRunId) {
            App.workflowDetail._selectedRunId = selectedRunId
            return App.api
              .get(App.workspacePath(`/workflow-runs/${encodeURIComponent(selectedRunId)}`))
              .then((runDetail) => {
                App.workflowDetail._selectedRunDetail = runDetail
                App.workflowDetail.renderEditor()
                App.workflowDetail.ensureRunPolling()
              })
          }

          App.workflowDetail._selectedRunDetail = null
          App.workflowDetail.renderEditor()
          App.workflowDetail.ensureRunPolling()
          return null
        })
        .catch((error) => {
          App.toast.error(`Failed to load workflow: ${error.message}`)
        })
    },

    ensureRunPolling() {
      if (App.workflowDetail._runTimer) {
        clearInterval(App.workflowDetail._runTimer)
        App.workflowDetail._runTimer = null
      }
      const detail = App.workflowDetail._selectedRunDetail
      const status = detail && detail.run ? detail.run.status : null
      if (!status || ['done', 'failed', 'cancelled'].includes(status)) return
      App.workflowDetail._runTimer = window.setInterval(() => {
        App.workflowDetail.load(false)
      }, 3000)
    },

    renderEditor() {
      const definition = App.workflowDetail._definition
      if (!definition) return

      const workflow = definition.workflow
      const nodes = definition.nodes || []
      const edges = definition.edges || []
      const runs = App.workflowDetail._runs || []
      const selectedRun = App.workflowDetail._selectedRunDetail
      const nodeMap = {}
      nodes.forEach((node) => {
        nodeMap[node.id] = node
      })

      let html = ''
      html += '<div style="margin-bottom:16px">'
      html += '<a href="#/workflows" style="color:var(--text-secondary);text-decoration:none;font-size:13px">&larr; Workflows</a>'
      html += '</div>'
      html += '<div class="page-header">'
      html += `<h2>${esc(workflow.name)}</h2>`
      html += '<div>'
      if (workflow.status !== 'published') {
        html += '<button class="btn btn-outline" onclick="App.workflowDetail.publish()">Publish</button> '
      }
      html += `<button class="btn btn-outline" onclick="App.workflowDetail.showRun()"${workflow.status === 'archived' ? ' disabled' : ''}>Run Workflow</button> `
      html += '<button class="btn btn-outline" onclick="App.workflowDetail.addNode(\'agent\')">+ Agent Step</button> '
      html += '<button class="btn btn-outline" onclick="App.workflowDetail.addNode(\'human_input\')">+ Human Input</button> '
      html += '<button class="btn btn-primary" onclick="App.workflowDetail.save()">Save</button> '
      html += `<button class="btn btn-danger" onclick="App.workflowDetail.remove('${escAttr(workflow.id)}')">Delete</button>`
      html += '</div></div>'

      html += '<div class="card" style="margin-bottom:20px"><div class="card-header">Workflow</div><div class="card-body">'
      html += '<div class="form-group"><label>Name</label><input id="wf-name" value="' + escAttr(workflow.name) + '"></div>'
      html += '<div class="form-group"><label>Description</label><textarea id="wf-description" placeholder="Optional description">' + esc(workflow.description || '') + '</textarea></div>'
      html += '<div class="form-row">'
      html += '<div class="form-group"><label>Status</label><select id="wf-status">'
      ;['draft', 'published', 'archived'].forEach((status) => {
        const selected = workflow.status === status ? ' selected' : ''
        html += `<option value="${status}"${selected}>${status}</option>`
      })
      html += '</select></div>'
      html += `<div class="form-group"><label>Workflow ID</label><input value="${escAttr(workflow.id)}" readonly onclick="this.select()" style="font-family:var(--mono)"></div>`
      html += '</div>'
      html += '</div></div>'

      html += '<div class="card" style="margin-bottom:20px"><div class="card-header">Flow Preview</div><div class="card-body">'
      if (!nodes.length) {
        html += '<p style="color:var(--text-secondary)">No nodes yet.</p>'
      } else {
        const preview = workflowPreviewLayout(definition)
        html += `<div class="workflow-canvas" style="height:${preview.height}px">`
        html += `<svg class="workflow-canvas-svg" viewBox="0 0 ${preview.width} ${preview.height}" preserveAspectRatio="xMinYMin meet">`
        html += '<defs><marker id="workflow-arrow" markerWidth="10" markerHeight="10" refX="9" refY="5" orient="auto"><path d="M 0 0 L 10 5 L 0 10 z" fill="#94a3b8"></path></marker></defs>'
        edges.forEach((edge) => {
          const from = preview.positions[edge.source_node_id]
          const to = preview.positions[edge.target_node_id]
          if (!from || !to) return
          html += `<path d="${workflowEdgePath(from, to, preview.nodeWidth, preview.nodeHeight)}" class="workflow-edge-line" marker-end="url(#workflow-arrow)"></path>`
        })
        html += '</svg>'
        nodes.forEach((node) => {
          const pos = preview.positions[node.id]
          if (!pos) return
          html += `<button type="button" class="workflow-node-card workflow-node-card-${escAttr(node.kind)}" style="left:${pos.x}px;top:${pos.y}px;width:${preview.nodeWidth}px" onclick="App.workflowDetail.focusNode('${escAttr(node.id)}')">`
          html += `<div class="workflow-node-card-kind">${esc(node.kind.replace('_', ' '))}</div>`
          html += `<div class="workflow-node-card-title">${esc(node.title || defaultNodeTitle(node.kind))}</div>`
          if (node.kind === 'agent' && node.agent_name) {
            html += `<div class="workflow-node-card-meta">${esc(node.agent_name)}</div>`
          } else if (node.kind === 'human_input') {
            html += '<div class="workflow-node-card-meta">user input</div>'
          } else if (node.kind === 'start') {
            html += '<div class="workflow-node-card-meta">run input</div>'
          } else if (node.kind === 'end') {
            html += '<div class="workflow-node-card-meta">terminal</div>'
          }
          html += '</button>'
        })
        html += '</div>'
      }
      html += '</div></div>'

      html += '<div class="card" style="margin-bottom:20px"><div class="card-header">Nodes</div><div class="card-body">'
      if (nodes.length === 0) {
        html += '<p style="color:var(--text-secondary)">No nodes.</p>'
      } else {
        nodes.forEach((node) => {
          html += `<div class="workflow-node-editor">`
          html += '<div class="workflow-node-head">'
          html += `<strong>${esc(node.title || defaultNodeTitle(node.kind))}</strong>`
          html += `<button class="btn btn-sm btn-danger" onclick="App.workflowDetail.removeNode('${escAttr(node.id)}')">Remove</button>`
          html += '</div>'
          html += '<div class="form-row">'
          html += `<div class="form-group"><label>Kind</label><select id="wf-node-kind-${escAttr(node.id)}" onchange="App.workflowDetail.changeKind('${escAttr(node.id)}', this.value)">`
          ;['start', 'agent', 'human_input', 'end'].forEach((kind) => {
            const selected = node.kind === kind ? ' selected' : ''
            html += `<option value="${kind}"${selected}>${kind}</option>`
          })
          html += '</select></div>'
          html += `<div class="form-group"><label>Title</label><input id="wf-node-title-${escAttr(node.id)}" value="${escAttr(node.title || '')}" placeholder="${escAttr(defaultNodeTitle(node.kind))}"></div>`
          html += '</div>'
          if (node.kind === 'agent') {
            html += `<div class="form-group"><label>Agent</label><select id="wf-node-agent-${escAttr(node.id)}">`
            App.workflowDetail._agents.forEach((agent) => {
              const selected = node.agent_name === agent.name ? ' selected' : ''
              html += `<option value="${escAttr(agent.name)}"${selected}>${esc(agent.name)}</option>`
            })
            html += '</select></div>'
          }
          html += `<div class="form-group"><label>${node.kind === 'human_input' ? 'Question / Prompt' : 'Prompt'}</label><textarea id="wf-node-prompt-${escAttr(node.id)}" placeholder="Optional prompt">${esc(node.prompt || '')}</textarea></div>`
          html += `<div class="workflow-node-meta">${esc(node.id)}</div>`
          html += '</div>'
        })
      }
      html += '</div></div>'

      html += '<div class="card"><div class="card-header">Edges</div><div class="card-body">'
      if (edges.length === 0) {
        html += '<p style="color:var(--text-secondary);margin-bottom:12px">No edges yet.</p>'
      } else {
        html += '<table style="margin-bottom:16px"><thead><tr><th>From</th><th>To</th><th></th></tr></thead><tbody>'
        edges.forEach((edge) => {
          const source = nodeMap[edge.source_node_id]
          const target = nodeMap[edge.target_node_id]
          html += '<tr>'
          html += `<td>${esc(source ? source.title : edge.source_node_id)}</td>`
          html += `<td>${esc(target ? target.title : edge.target_node_id)}</td>`
          html += `<td><button class="btn btn-sm btn-danger" onclick="App.workflowDetail.removeEdge('${escAttr(edge.id)}')">Remove</button></td>`
          html += '</tr>'
        })
        html += '</tbody></table>'
      }
      html += '<div class="form-row">'
      html += '<div class="form-group"><label>From</label><select id="wf-new-edge-source">'
      nodes.forEach((node) => {
        html += `<option value="${escAttr(node.id)}">${esc(node.title || defaultNodeTitle(node.kind))}</option>`
      })
      html += '</select></div>'
      html += '<div class="form-group"><label>To</label><select id="wf-new-edge-target">'
      nodes.forEach((node) => {
        html += `<option value="${escAttr(node.id)}">${esc(node.title || defaultNodeTitle(node.kind))}</option>`
      })
      html += '</select></div>'
      html += '</div>'
      html += '<button class="btn btn-outline" onclick="App.workflowDetail.addEdge()">+ Add Edge</button>'
      html += '</div></div>'

      html += '<div class="card" style="margin-top:20px;margin-bottom:20px"><div class="card-header">Runs'
      if (runs.length) {
        html += `<span style="font-weight:normal;color:var(--text-secondary);margin-left:8px">${runs.length}</span>`
      }
      html += '</div><div class="card-body">'
      if (!runs.length) {
        html += '<div class="empty-state" style="padding:24px 12px"><p>No workflow runs yet.</p>'
        html += `<button class="btn btn-primary" onclick="App.workflowDetail.showRun()"${workflow.status === 'archived' ? ' disabled' : ''}>Run This Workflow</button>`
        html += '</div>'
      } else {
        html += '<table><thead><tr><th>Run</th><th>Status</th><th>Started</th><th>Finished</th><th></th></tr></thead><tbody>'
        runs.forEach((run) => {
          const selected = App.workflowDetail._selectedRunId === run.id ? ' style="background:rgba(59,130,246,0.06)"' : ''
          html += `<tr class="clickable"${selected} onclick="App.workflowDetail.selectRun('${escAttr(run.id)}')">`
          html += `<td><strong>${esc(run.id)}</strong></td>`
          html += `<td>${statusDot(run.status)}</td>`
          html += `<td>${timeAgo(run.started_at)}</td>`
          html += `<td>${run.finished_at ? timeAgo(run.finished_at) : '<span style="color:var(--text-secondary)">running</span>'}</td>`
          html += '<td>'
          if (run.id === App.workflowDetail._selectedRunId) {
            html += '<span style="color:var(--text-secondary)">selected</span>'
          }
          html += '</td></tr>'
        })
        html += '</tbody></table>'
      }
      html += '</div></div>'

      if (selectedRun && selectedRun.run) {
        const stepRuns = selectedRun.step_runs || []
        html += '<div class="card"><div class="card-header">Run Detail</div><div class="card-body">'
        html += '<dl class="detail-grid" style="margin-bottom:20px">'
        html += `<dt>Run ID</dt><dd>${esc(selectedRun.run.id)}</dd>`
        html += `<dt>Status</dt><dd>${statusDot(selectedRun.run.status)}</dd>`
        html += `<dt>Started</dt><dd>${timeAgo(selectedRun.run.started_at)}</dd>`
        html += `<dt>Input</dt><dd>${selectedRun.run.input ? `<div class="instructions-block">${esc(selectedRun.run.input)}</div>` : '<span style="color:var(--text-secondary)">(none)</span>'}</dd>`
        if (selectedRun.run.error) {
          html += `<dt>Error</dt><dd><div class="instructions-block">${esc(selectedRun.run.error)}</div></dd>`
        }
        html += '</dl>'
        if (!stepRuns.length) {
          html += '<p style="color:var(--text-secondary)">No step runs.</p>'
        } else {
          stepRuns.forEach((step) => {
            html += '<div class="workflow-step-run">'
            html += '<div class="workflow-step-run-head">'
            html += `<div><strong>${esc(step.node_title)}</strong><div style="font-size:12px;color:var(--text-secondary)">${esc(step.node_kind)}${step.agent_name ? ` · ${esc(step.agent_name)}` : ''}</div></div>`
            html += '<div>'
            html += `${statusDot(step.status)}`
            if (['done', 'failed'].includes(step.status)) {
              html += ` <button class="btn btn-sm btn-outline" onclick="App.workflowDetail.retryStep('${escAttr(selectedRun.run.id)}','${escAttr(step.id)}')">Retry</button>`
            }
            if (step.status === 'waiting_for_input') {
              html += ` <button class="btn btn-sm btn-primary" onclick="App.workflowDetail.showStepInput('${escAttr(selectedRun.run.id)}','${escAttr(step.id)}')">Provide Input</button>`
            }
            if (step.thread_id) {
              const label = App.workflowDetail._expandedThreadId === step.thread_id ? 'Hide Messages' : 'View Messages'
              html += ` <button class="btn btn-sm btn-outline" onclick="App.workflowDetail.toggleStepThread('${escAttr(step.thread_id)}')">${label}</button>`
            }
            html += '</div></div>'
            if (step.input) {
              html += `<div class="workflow-step-run-block"><div class="workflow-step-run-label">Input</div><div class="instructions-block">${esc(step.input)}</div></div>`
            }
            if (step.output) {
              html += `<div class="workflow-step-run-block"><div class="workflow-step-run-label">Output</div><div class="instructions-block">${esc(step.output)}</div></div>`
            }
            if (step.error) {
              html += `<div class="workflow-step-run-block"><div class="workflow-step-run-label">Error</div><div class="instructions-block">${esc(step.error)}</div></div>`
            }
            if (step.thread_id && App.workflowDetail._expandedThreadId === step.thread_id) {
              html += App.workflowDetail.renderThreadMessages(step.thread_id)
            }
            html += '</div>'
          })
        }
        html += '</div></div>'
      }

      setContent(html)
    },

    focusNode(nodeId) {
      const input = document.getElementById(`wf-node-title-${nodeId}`)
      if (!input) return
      input.scrollIntoView({ behavior: 'smooth', block: 'center' })
      input.focus()
      input.select()
    },

    selectRun(runId) {
      App.workflowDetail._selectedRunId = runId
      App.workflowDetail._expandedThreadId = null
      App.api
        .get(App.workspacePath(`/workflow-runs/${encodeURIComponent(runId)}`))
        .then((runDetail) => {
          App.workflowDetail._selectedRunDetail = runDetail
          App.workflowDetail.renderEditor()
          App.workflowDetail.ensureRunPolling()
        })
        .catch((error) => {
          App.toast.error(`Failed to load run: ${error.message}`)
        })
    },

    renderThreadMessages(threadId) {
      const messages = App.workflowDetail._threadMessages[threadId]
      if (!messages) {
        return '<div class="workflow-thread-box"><div class="loading"><span class="spinner"></span></div></div>'
      }
      if (messages.length === 0) {
        return '<div class="workflow-thread-box"><p style="color:var(--text-secondary)">No messages yet.</p></div>'
      }

      let html = '<div class="workflow-thread-box"><div class="thread-messages">'
      messages.forEach((message) => {
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
      html += '</div></div>'
      return html
    },

    toggleStepThread(threadId) {
      if (App.workflowDetail._expandedThreadId === threadId) {
        App.workflowDetail._expandedThreadId = null
        App.workflowDetail.renderEditor()
        return
      }

      App.workflowDetail._expandedThreadId = threadId
      App.workflowDetail.renderEditor()
      App.api
        .get(App.workspacePath(`/threads/${encodeURIComponent(threadId)}`))
        .then((data) => {
          App.workflowDetail._threadMessages[threadId] = data.messages || []
          App.workflowDetail.renderEditor()
        })
        .catch((error) => {
          App.toast.error(`Failed to load messages: ${error.message}`)
        })
    },

    showRun() {
      const workflow = App.workflowDetail._definition && App.workflowDetail._definition.workflow
      if (!workflow || workflow.status === 'archived') return
      let html = '<div class="modal-overlay" onclick="if(event.target===this)this.remove()"><div class="modal">'
      html += '<div class="modal-header">Run Workflow<button class="btn-icon" onclick="this.closest(\'.modal-overlay\').remove()">&times;</button></div>'
      html += '<div class="modal-body">'
      html += '<div class="form-group"><label>Run Input</label><textarea id="wf-run-input" placeholder="Optional instructions or context for this run"></textarea></div>'
      html += '</div>'
      html += '<div class="modal-footer"><button class="btn btn-outline" onclick="this.closest(\'.modal-overlay\').remove()">Cancel</button>'
      html += '<button class="btn btn-primary" onclick="App.workflowDetail.startRun()">Start Run</button></div></div></div>'
      document.body.insertAdjacentHTML('beforeend', html)
      document.getElementById('wf-run-input').focus()
    },

    startRun() {
      const workflow = App.workflowDetail._definition && App.workflowDetail._definition.workflow
      if (!workflow) return
      const input = document.getElementById('wf-run-input').value.trim()
      App.api
        .post(App.workspacePath(`/workflows/${encodeURIComponent(workflow.id)}/runs`), {
          input: input || null,
        })
        .then((runDetail) => {
          document.querySelector('.modal-overlay').remove()
          App.toast.success('Workflow run started')
          App.workflowDetail._selectedRunId = runDetail.run.id
          App.workflowDetail._selectedRunDetail = runDetail
          App.workflowDetail.load(false)
        })
        .catch((error) => {
          App.toast.error(`Failed: ${error.message}`)
        })
    },

    publish() {
      const workflow = App.workflowDetail._definition && App.workflowDetail._definition.workflow
      if (!workflow) return
      App.api
        .post(App.workspacePath(`/workflows/${encodeURIComponent(workflow.id)}/publish`))
        .then((updated) => {
          App.workflowDetail._definition = updated
          App.toast.success('Workflow published')
          App.workflowDetail.renderEditor()
        })
        .catch((error) => {
          App.toast.error(`Failed: ${error.message}`)
        })
    },

    changeKind(nodeId, kind) {
      const node = (App.workflowDetail._definition.nodes || []).find((item) => item.id === nodeId)
      if (!node) return
      node.kind = kind
      if (kind !== 'agent') node.agent_name = null
      if (!node.title) node.title = defaultNodeTitle(kind)
      App.workflowDetail.renderEditor()
    },

    addNode(kind) {
      const definition = App.workflowDetail._definition
      if (!definition) return
      definition.nodes.push({
        id: `node-${crypto.randomUUID().slice(0, 8)}`,
        workflow_id: definition.workflow.id,
        kind,
        title: defaultNodeTitle(kind),
        prompt: '',
        agent_name: kind === 'agent' && App.workflowDetail._agents[0] ? App.workflowDetail._agents[0].name : null,
        position_x: 0,
        position_y: 0,
      })
      App.workflowDetail.renderEditor()
    },

    removeNode(nodeId) {
      const definition = App.workflowDetail._definition
      if (!definition) return
      definition.nodes = (definition.nodes || []).filter((node) => node.id !== nodeId)
      definition.edges = (definition.edges || []).filter(
        (edge) => edge.source_node_id !== nodeId && edge.target_node_id !== nodeId,
      )
      App.workflowDetail.renderEditor()
    },

    addEdge() {
      const source = document.getElementById('wf-new-edge-source').value
      const target = document.getElementById('wf-new-edge-target').value
      if (!source || !target) {
        App.toast.error('Select both nodes')
        return
      }
      if (source === target) {
        App.toast.error('An edge cannot point to the same node')
        return
      }

      const exists = (App.workflowDetail._definition.edges || []).some(
        (edge) => edge.source_node_id === source && edge.target_node_id === target,
      )
      if (exists) {
        App.toast.error('That edge already exists')
        return
      }

      App.workflowDetail._definition.edges.push({
        id: `edge-${crypto.randomUUID().slice(0, 8)}`,
        workflow_id: App.workflowDetail._definition.workflow.id,
        source_node_id: source,
        target_node_id: target,
      })
      App.workflowDetail.renderEditor()
    },

    removeEdge(edgeId) {
      App.workflowDetail._definition.edges = (App.workflowDetail._definition.edges || []).filter((edge) => edge.id !== edgeId)
      App.workflowDetail.renderEditor()
    },

    showStepInput(runId, stepRunId) {
      let html = '<div class="modal-overlay" onclick="if(event.target===this)this.remove()"><div class="modal">'
      html += '<div class="modal-header">Provide Input<button class="btn-icon" onclick="this.closest(\'.modal-overlay\').remove()">&times;</button></div>'
      html += '<div class="modal-body">'
      html += '<div class="form-group"><label>Input</label><textarea id="wf-step-input" placeholder="Enter the response or approval text"></textarea></div>'
      html += '</div>'
      html += '<div class="modal-footer"><button class="btn btn-outline" onclick="this.closest(\'.modal-overlay\').remove()">Cancel</button>'
      html += `<button class="btn btn-primary" onclick="App.workflowDetail.submitStepInput('${escAttr(runId)}','${escAttr(stepRunId)}')">Submit</button></div></div></div>`
      document.body.insertAdjacentHTML('beforeend', html)
      document.getElementById('wf-step-input').focus()
    },

    submitStepInput(runId, stepRunId) {
      const input = document.getElementById('wf-step-input').value.trim()
      if (!input) {
        App.toast.error('Input is required')
        return
      }
      App.api
        .post(App.workspacePath(`/workflow-runs/${encodeURIComponent(runId)}/steps/${encodeURIComponent(stepRunId)}/input`), {
          input,
        })
        .then((runDetail) => {
          document.querySelector('.modal-overlay').remove()
          App.toast.success('Input submitted')
          App.workflowDetail._selectedRunId = runDetail.run.id
          App.workflowDetail._selectedRunDetail = runDetail
          App.workflowDetail.load(false)
        })
        .catch((error) => {
          App.toast.error(`Failed: ${error.message}`)
        })
    },

    retryStep(runId, stepRunId) {
      App.api
        .post(App.workspacePath(`/workflow-runs/${encodeURIComponent(runId)}/steps/${encodeURIComponent(stepRunId)}/retry`))
        .then((runDetail) => {
          App.toast.success('Step retried')
          App.workflowDetail._selectedRunId = runDetail.run.id
          App.workflowDetail._selectedRunDetail = runDetail
          App.workflowDetail.load(false)
        })
        .catch((error) => {
          App.toast.error(`Failed: ${error.message}`)
        })
    },

    serializeNodes() {
      return (App.workflowDetail._definition.nodes || []).map((node) => {
        const kind = document.getElementById(`wf-node-kind-${node.id}`).value
        const title = document.getElementById(`wf-node-title-${node.id}`).value.trim()
        const promptEl = document.getElementById(`wf-node-prompt-${node.id}`)
        const agentEl = document.getElementById(`wf-node-agent-${node.id}`)
        return {
          id: node.id,
          kind,
          title,
          prompt: promptEl ? promptEl.value.trim() : '',
          agent_name: kind === 'agent' && agentEl ? agentEl.value : null,
          position_x: node.position_x || 0,
          position_y: node.position_y || 0,
        }
      })
    },

    save() {
      const definition = App.workflowDetail._definition
      if (!definition) return
      const payload = {
        name: document.getElementById('wf-name').value.trim(),
        description: document.getElementById('wf-description').value.trim(),
        status: document.getElementById('wf-status').value,
        nodes: App.workflowDetail.serializeNodes(),
        edges: (definition.edges || []).map((edge) => ({
          id: edge.id,
          source_node_id: edge.source_node_id,
          target_node_id: edge.target_node_id,
        })),
      }

      App.api
        .put(App.workspacePath(`/workflows/${encodeURIComponent(definition.workflow.id)}`), payload)
        .then((updated) => {
          App.toast.success('Workflow saved')
          App.workflowDetail.refreshAfterSave(updated)
        })
        .catch((error) => {
          App.toast.error(`Failed: ${error.message}`)
        })
    },

    refreshAfterSave(updated) {
      App.workflowDetail._definition = updated
      App.workflowDetail.load(false)
    },

    remove(workflowId) {
      if (!confirm('Remove this workflow?')) return
      App.api
        .del(App.workspacePath(`/workflows/${encodeURIComponent(workflowId)}`))
        .then(() => {
          App.toast.success('Workflow removed')
          App.router.navigate('#/workflows')
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
