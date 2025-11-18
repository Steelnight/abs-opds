import { InternalUser } from '../types/internal'
import type { Request, Response } from 'express'
import axios from 'axios'
import { serverURL, useProxy } from '../index'

export async function apiCall(path: string, user: InternalUser) {
    const request = await axios.get(serverURL + '/api' + path, {
        headers: {
            Authorization: `Bearer ${user.apiKey}`
        }
    })

    if (request.status !== 200) {
        throw new Error(`Error: ${request.status} ${request.statusText}`)
    }

    return request.data
}

export async function proxyToAudiobookshelf(req: Request, res: Response) {
    if (process.env.NODE_ENV === 'development') {
        console.log(`[DEBUG] Attempting ABS proxy for request: ${req.originalUrl}`)
    }

    if (!useProxy) {
        res.status(403).send('Forbidden')
        return
    }

    if (req.method !== 'GET') {
        res.status(405).send('Method Not Allowed')
        return
    }

    try {
        const target = new URL(req.originalUrl.replace(/^\/opds\/proxy/, ''), serverURL).toString()

        const response = await axios.get(target, {
            responseType: 'stream',
            headers: {
                'x-forwarded-proto': req.protocol,
                'x-forwarded-host': req.get('host') ?? ''
            },
            maxRedirects: 0,
            timeout: 15000,
            validateStatus: () => true
        })

        res.status(response.status)
        for (const [key, value] of Object.entries(response.headers)) {
            if (value !== undefined) {
                res.setHeader(key, value as any)
            }
        }

        response.data.pipe(res)
        response.data.on('error', () => {
            if (!res.headersSent) res.status(502)
            res.end()
        })
    } catch (err) {
        if (process.env.NODE_ENV === 'development') {
            console.error('[DEBUG] ABS proxy error:', err)
        }
        if (!res.headersSent) {
            res.status(502).send('Bad Gateway')
        } else {
            res.end()
        }
    }
}
