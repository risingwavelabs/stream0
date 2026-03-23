import { createFileRoute, useNavigate } from '@tanstack/react-router'
import * as React from 'react'
import type { EmailOtpType } from '@supabase/supabase-js'
import { supabase } from '~/lib/supabase'

export const Route = createFileRoute('/login')({
  component: LoginPage,
})

function LoginPage() {
  const navigate = useNavigate()
  const [email, setEmail] = React.useState('')
  const [password, setPassword] = React.useState('')
  const [error, setError] = React.useState<string | null>(null)
  const [notice, setNotice] = React.useState<string | null>(null)
  const [loadingMode, setLoadingMode] = React.useState<
    'password' | 'magic-link' | 'verifying' | null
  >(null)
  const [oauthLoading, setOauthLoading] = React.useState<
    'google' | 'github' | null
  >(null)

  React.useEffect(() => {
    let mounted = true

    supabase.auth.getSession().then(({ data: { session } }) => {
      if (session && mounted) {
        navigate({ to: '/tasks' })
      }
    })

    const {
      data: { subscription },
    } = supabase.auth.onAuthStateChange((_event, session) => {
      if (session && mounted) {
        navigate({ to: '/tasks' })
      }
    })

    return () => {
      mounted = false
      subscription.unsubscribe()
    }
  }, [navigate])

  React.useEffect(() => {
    const params = new URLSearchParams(window.location.search)
    const tokenHash = params.get('token_hash')
    const type = params.get('type') as EmailOtpType | null

    if (!tokenHash) return

    setLoadingMode('verifying')
    supabase.auth
      .verifyOtp({
        token_hash: tokenHash,
        type: type ?? 'email',
      })
      .then(({ error: verifyError }) => {
        window.history.replaceState({}, '', window.location.pathname)
        if (verifyError) {
          setError(verifyError.message)
          setLoadingMode(null)
          return
        }

        navigate({ to: '/tasks' })
      })
  }, [navigate])

  const onPasswordSubmit = async () => {
    const nextEmail = email.trim().toLowerCase()
    if (!nextEmail || !password.trim()) {
      setError('Please enter your email and password.')
      return
    }

    setError(null)
    setNotice(null)
    setLoadingMode('password')

    const { error: loginError } = await supabase.auth.signInWithPassword({
      email: nextEmail,
      password,
    })

    if (loginError) {
      setError(loginError.message)
      setLoadingMode(null)
      return
    }

    navigate({ to: '/tasks' })
  }

  const onMagicLinkSubmit = async () => {
    const nextEmail = email.trim().toLowerCase()
    if (!nextEmail) {
      setError('Please enter your email before requesting a magic link.')
      return
    }

    setError(null)
    setNotice(null)
    setLoadingMode('magic-link')

    const { error: otpError } = await supabase.auth.signInWithOtp({
      email: nextEmail,
      options: {
        shouldCreateUser: true,
        emailRedirectTo:
          typeof window !== 'undefined'
            ? `${window.location.origin}/login`
            : '/login',
      },
    })

    if (otpError) {
      setError(otpError.message)
      setLoadingMode(null)
      return
    }

    setNotice('Magic link sent. Please check your inbox.')
    setLoadingMode(null)
  }

  const onOAuthSubmit = async (provider: 'google' | 'github') => {
    setError(null)
    setNotice(null)
    setOauthLoading(provider)

    const { error: oauthError } = await supabase.auth.signInWithOAuth({
      provider,
      options: {
        redirectTo:
          typeof window !== 'undefined'
            ? `${window.location.origin}/login`
            : '/login',
        queryParams: provider === 'google' ? { prompt: 'select_account' } : {},
      },
    })

    if (oauthError) {
      setError(oauthError.message)
      setOauthLoading(null)
    }
  }

  const isBusy = loadingMode !== null || oauthLoading !== null

  return (
    <div className="login-page">
      <div className="login-aura login-aura-left" aria-hidden />
      <div className="login-aura login-aura-right" aria-hidden />
      <div className="login-grid">
        <section className="login-panel login-brand-panel">
          <div className="login-chip">Box0 Control Plane</div>
          <h1>Run autonomous teams with confidence</h1>
          <p>
            Unified sign-in for your Box0 workspace. Secure Supabase auth,
            collaborative orchestration, and full project visibility in one
            place.
          </p>
          <div className="login-feature-list">
            <div className="login-feature-item">Realtime task and agent status</div>
            <div className="login-feature-item">
              Role-ready authentication and token flow
            </div>
            <div className="login-feature-item">
              One dashboard for operators, builders, and reviewers
            </div>
          </div>
        </section>

        <section className="login-panel login-box">
          <div className="login-form-header">
            <h2>Welcome back</h2>
            <p>Sign in with your Supabase account to continue.</p>
          </div>

          {error ? (
            <div className="login-error" style={{ display: 'block' }}>
              {error}
            </div>
          ) : (
            <div className="login-error" />
          )}
          {notice ? (
            <div className="login-success" style={{ display: 'block' }}>
              {notice}
            </div>
          ) : (
            <div className="login-success" />
          )}

          <label className="login-label" htmlFor="login-email">
            Email
          </label>
          <input
            id="login-email"
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && void onPasswordSubmit()}
            placeholder="you@company.com"
            autoComplete="email"
          />

          <label className="login-label" htmlFor="login-password">
            Password
          </label>
          <input
            id="login-password"
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && void onPasswordSubmit()}
            placeholder="Your password"
            autoComplete="current-password"
          />

          <button
            type="button"
            className="btn btn-primary login-cta"
            onClick={() => void onPasswordSubmit()}
            disabled={isBusy}
          >
            {loadingMode === 'password' ? 'Signing in...' : 'Sign in'}
          </button>
          <button
            type="button"
            className="btn btn-outline login-cta"
            onClick={() => void onMagicLinkSubmit()}
            disabled={isBusy}
          >
            {loadingMode === 'magic-link'
              ? 'Sending magic link...'
              : 'Send magic link'}
          </button>

          <div className="login-divider">or continue with</div>
          <button
            type="button"
            className="btn btn-outline login-cta"
            onClick={() => void onOAuthSubmit('google')}
            disabled={isBusy}
          >
            {oauthLoading === 'google'
              ? 'Redirecting to Google...'
              : 'Continue with Google'}
          </button>
          <button
            type="button"
            className="btn btn-outline login-cta"
            onClick={() => void onOAuthSubmit('github')}
            disabled={isBusy}
          >
            {oauthLoading === 'github'
              ? 'Redirecting to GitHub...'
              : 'Continue with GitHub'}
          </button>

          {loadingMode === 'verifying' ? (
            <p className="login-hint">Verifying magic link...</p>
          ) : (
            <p className="login-hint">
              Need first-time access? Use magic link or OAuth to bootstrap.
            </p>
          )}
        </section>
      </div>
    </div>
  )
}
