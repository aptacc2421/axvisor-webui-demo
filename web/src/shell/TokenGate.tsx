//! token 门。token 只存在内存态（useState），不落 sessionStorage/localStorage。

import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'

export function TokenGate({ onSubmit }: { onSubmit: (token: string) => void }) {
  const [value, setValue] = useState('')

  return (
    <div className="flex min-h-screen items-center justify-center bg-muted/40 p-4">
      <Card className="w-full max-w-sm">
        <CardHeader>
          <CardTitle>webui_demo</CardTitle>
          <CardDescription>
            传输层统一鉴权的占位：固定 token <code className="font-mono">demo-token</code>
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form
            className="flex gap-2"
            onSubmit={(e) => {
              e.preventDefault()
              const token = value.trim()
              if (token) onSubmit(token)
            }}
          >
            <Input
              autoFocus
              value={value}
              onChange={(e) => setValue(e.target.value)}
              placeholder="Bearer token"
              aria-label="token"
            />
            <Button type="submit" disabled={!value.trim()}>
              进入
            </Button>
          </form>
        </CardContent>
      </Card>
    </div>
  )
}
