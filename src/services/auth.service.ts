import { getPrisma } from '../lib/prismaScope.js'
import { hashPassword, comparePassword, generateAccessToken, generateRefreshToken, verifyRefreshToken, hashToken } from '../lib/auth-utils.js'
import { RegisterInput, LoginInput } from '../lib/validation.js'
import { UserRole } from '../types/user.js'
import { randomUUID } from 'node:crypto'
import { recordSession, revokeAllUserSessions } from './session.js'

export class AuthService {
    static async register(input: RegisterInput) {
        try {
            const hashedPassword = await hashPassword(input.password)
            const user = await getPrisma().user.create({
                data: {
                    email: input.email,
                    passwordHash: hashedPassword,
                    role: input.role || UserRole.USER,
                },
            })

            return { id: user.id, email: user.email, role: user.role }
        } catch (error: any) {
            if (error.code === 'P2002') {
                throw new Error('Email already in use')
            }
            throw error
        }
    }

    static async login(input: LoginInput) {
        const user = await getPrisma().user.findUnique({ where: { email: input.email } })
        if (!user) {
            throw new Error('Invalid credentials')
        }

        const isValid = await comparePassword(input.password, user.passwordHash)
        if (!isValid) {
            throw new Error('Invalid credentials')
        }

        await getPrisma().user.update({
            where: { id: user.id },
            data: { lastLoginAt: new Date() },
        })

        const jti = randomUUID()
        const accessToken = generateAccessToken({ userId: user.id, role: user.role, jti })
        const refreshTokenValue = generateRefreshToken({ userId: user.id })

        // 1. Record session for access token (middleware/auth.ts compatibility)
        const accessExpiresAt = new Date(Date.now() + 15 * 60 * 1000) // 15 minutes
        await recordSession(user.id, jti, accessExpiresAt)

        // 2. Store hashed refresh token — the raw value is only returned to the client
        const tokenHash = hashToken(refreshTokenValue)
        await getPrisma().refreshToken.create({
            data: {
                token: tokenHash,
                userId: user.id,
                expiresAt: new Date(Date.now() + 7 * 24 * 60 * 60 * 1000), // 7 days
            },
        })

        return {
            user: { id: user.id, email: user.email, role: user.role },
            accessToken,
            refreshToken: refreshTokenValue,
        }
    }

    static async refresh(token: string) {
        try {
            const payload = verifyRefreshToken(token)

            // Look up by hash — we never store the raw token
            const tokenHash = hashToken(token)
            const storedToken = await getPrisma().refreshToken.findUnique({
                where: { token: tokenHash },
                include: { user: true },
            })

            if (!storedToken || storedToken.revokedAt || storedToken.expiresAt < new Date()) {
                throw new Error('Invalid or expired refresh token')
            }

            // Revoke old token BEFORE issuing new ones (no dual-valid window)
            await getPrisma().refreshToken.update({
                where: { id: storedToken.id },
                data: { revokedAt: new Date() },
            })

            const jti = randomUUID()
            const newAccessToken = generateAccessToken({ userId: storedToken.user.id, role: storedToken.user.role, jti })
            const newRefreshTokenValue = generateRefreshToken({ userId: storedToken.user.id })

            // 1. Record new session for access token
            const accessExpiresAt = new Date(Date.now() + 15 * 60 * 1000)
            await recordSession(storedToken.user.id, jti, accessExpiresAt)

            // 2. Store hashed new refresh token
            const newTokenHash = hashToken(newRefreshTokenValue)
            await getPrisma().refreshToken.create({
                data: {
                    token: newTokenHash,
                    userId: storedToken.user.id,
                    expiresAt: new Date(Date.now() + 7 * 24 * 60 * 60 * 1000),
                },
            })

            return {
                accessToken: newAccessToken,
                refreshToken: newRefreshTokenValue,
            }
        } catch (error) {
            throw new Error('Invalid refresh token')
        }
    }

    static async logout(token: string) {
        const tokenHash = hashToken(token)
        await getPrisma().refreshToken.updateMany({
            where: { token: tokenHash },
            data: { revokedAt: new Date() },
        })
    }

    /**
     * Revoke ALL refresh tokens AND sessions for a user.
     * Used by the /logout-all endpoint.
     */
    static async logoutAll(userId: string) {
        // 1. Revoke all refresh tokens for this user
        await getPrisma().refreshToken.updateMany({
            where: { userId, revokedAt: null },
            data: { revokedAt: new Date() },
        })

        // 2. Revoke all access token sessions
        await revokeAllUserSessions(userId)
    }
}

