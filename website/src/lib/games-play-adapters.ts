import type { ComponentType } from "react";

/**
 * The integration point for game-specific play interfaces.
 *
 * A game's results are judged by an approved verifier module, identified by its SHA3-256 hash. What a player
 * actually does (moves a piece, routes a signal) is turned into the bytes that module judges by a game-specific
 * interface. A game ships that interface by registering a component here under its verifier's hash; the Games
 * pages then show it in place of the developer console.
 *
 * Nothing is registered yet: the catalog has no game with a public interface. Until one exists, a playable game
 * offers only the clearly labelled developer input, never a pretend game.
 */
export interface GameInterfaceProps {
  /** True while a play is being judged or no wallet session exists. */
  disabled: boolean;
  /** Called with the exact input bytes for the verifier. The service judges them; the interface never decides. */
  onSubmit(payload: Uint8Array): void;
}

export type GameInterface = ComponentType<GameInterfaceProps>;

const interfaces = new Map<string, GameInterface>();

export function registerGameInterface(adapterHash: string, component: GameInterface): void {
  if (!/^[0-9a-f]{64}$/.test(adapterHash)) throw new Error("A verifier hash is 64 lowercase hexadecimal characters.");
  interfaces.set(adapterHash, component);
}

export function gameInterfaceFor(adapterHash: string | null): GameInterface | null {
  return adapterHash ? (interfaces.get(adapterHash) ?? null) : null;
}

export function bytesToPayloadHex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

/** The raw input a developer types, as hex. Even length, at most 4,096 bytes. Null means acceptable. */
export function payloadHexError(text: string): string | null {
  if (text.length === 0) return "Enter the input as hexadecimal, for example 01.";
  if (text.length % 2 !== 0) return "Hexadecimal comes in pairs of characters.";
  if (text.length > 8_192) return "That is longer than a game input may be (4,096 bytes).";
  if (!/^[0-9a-fA-F]+$/.test(text)) return "Use only 0 to 9 and a to f.";
  return null;
}
