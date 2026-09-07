/* @ts-self-types="./zcash_wallet_wasm.d.ts" */
import { startWorkers } from './snippets/wasm-bindgen-rayon-38edf6e439f6d70d/src/workerHelpers.js';


export class BlockRange {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        BlockRangeFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_blockrange_free(ptr, 0);
    }
    /**
     * @returns {number}
     */
    get 0() {
        const ret = wasm.__wbg_get_blockrange_0(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    get 1() {
        const ret = wasm.__wbg_get_blockrange_1(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @param {number} arg0
     */
    set 0(arg0) {
        wasm.__wbg_set_blockrange_0(this.__wbg_ptr, arg0);
    }
    /**
     * @param {number} arg0
     */
    set 1(arg0) {
        wasm.__wbg_set_blockrange_1(this.__wbg_ptr, arg0);
    }
}
if (Symbol.dispose) BlockRange.prototype[Symbol.dispose] = BlockRange.prototype.free;

export class IntoUnderlyingByteSource {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        IntoUnderlyingByteSourceFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_intounderlyingbytesource_free(ptr, 0);
    }
    /**
     * @returns {number}
     */
    get autoAllocateChunkSize() {
        const ret = wasm.intounderlyingbytesource_autoAllocateChunkSize(this.__wbg_ptr);
        return ret >>> 0;
    }
    cancel() {
        const ptr = this.__destroy_into_raw();
        wasm.intounderlyingbytesource_cancel(ptr);
    }
    /**
     * @param {ReadableByteStreamController} controller
     * @returns {Promise<any>}
     */
    pull(controller) {
        const ret = wasm.intounderlyingbytesource_pull(this.__wbg_ptr, addHeapObject(controller));
        return takeObject(ret);
    }
    /**
     * @param {ReadableByteStreamController} controller
     */
    start(controller) {
        wasm.intounderlyingbytesource_start(this.__wbg_ptr, addHeapObject(controller));
    }
    /**
     * @returns {ReadableStreamType}
     */
    get type() {
        const ret = wasm.intounderlyingbytesource_type(this.__wbg_ptr);
        return __wbindgen_enum_ReadableStreamType[ret];
    }
}
if (Symbol.dispose) IntoUnderlyingByteSource.prototype[Symbol.dispose] = IntoUnderlyingByteSource.prototype.free;

export class IntoUnderlyingSink {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        IntoUnderlyingSinkFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_intounderlyingsink_free(ptr, 0);
    }
    /**
     * @param {any} reason
     * @returns {Promise<any>}
     */
    abort(reason) {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.intounderlyingsink_abort(ptr, addHeapObject(reason));
        return takeObject(ret);
    }
    /**
     * @returns {Promise<any>}
     */
    close() {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.intounderlyingsink_close(ptr);
        return takeObject(ret);
    }
    /**
     * @param {any} chunk
     * @returns {Promise<any>}
     */
    write(chunk) {
        const ret = wasm.intounderlyingsink_write(this.__wbg_ptr, addHeapObject(chunk));
        return takeObject(ret);
    }
}
if (Symbol.dispose) IntoUnderlyingSink.prototype[Symbol.dispose] = IntoUnderlyingSink.prototype.free;

export class IntoUnderlyingSource {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        IntoUnderlyingSourceFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_intounderlyingsource_free(ptr, 0);
    }
    cancel() {
        const ptr = this.__destroy_into_raw();
        wasm.intounderlyingsource_cancel(ptr);
    }
    /**
     * @param {ReadableStreamDefaultController} controller
     * @returns {Promise<any>}
     */
    pull(controller) {
        const ret = wasm.intounderlyingsource_pull(this.__wbg_ptr, addHeapObject(controller));
        return takeObject(ret);
    }
}
if (Symbol.dispose) IntoUnderlyingSource.prototype[Symbol.dispose] = IntoUnderlyingSource.prototype.free;

export class PaymentRequest {
    static __wrap(ptr) {
        const obj = Object.create(PaymentRequest.prototype);
        obj.__wbg_ptr = ptr;
        PaymentRequestFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    static __unwrap(jsValue) {
        if (!(jsValue instanceof PaymentRequest)) {
            return 0;
        }
        return jsValue.__destroy_into_raw();
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        PaymentRequestFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_paymentrequest_free(ptr, 0);
    }
    /**
     * @returns {bigint}
     */
    amount() {
        const ret = wasm.paymentrequest_amount(this.__wbg_ptr);
        return BigInt.asUintN(64, ret);
    }
    /**
     * @returns {string | undefined}
     */
    label() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.paymentrequest_label(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            let v1;
            if (r0 !== 0) {
                v1 = getStringFromWasm0(r0, r1).slice();
                wasm.__wbindgen_export4(r0, r1 * 1, 1);
            }
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @returns {Uint8Array | undefined}
     */
    memo() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.paymentrequest_memo(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            let v1;
            if (r0 !== 0) {
                v1 = getArrayU8FromWasm0(r0, r1).slice();
                wasm.__wbindgen_export4(r0, r1 * 1, 1);
            }
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @returns {string | undefined}
     */
    message() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.paymentrequest_message(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            let v1;
            if (r0 !== 0) {
                v1 = getStringFromWasm0(r0, r1).slice();
                wasm.__wbindgen_export4(r0, r1 * 1, 1);
            }
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @param {string} recipient_address
     * @param {bigint} amount
     * @param {Uint8Array | null | undefined} memo
     * @param {string | null | undefined} label
     * @param {string | null | undefined} message
     * @param {any} other_params
     */
    constructor(recipient_address, amount, memo, label, message, other_params) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(recipient_address, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            var ptr1 = isLikeNone(memo) ? 0 : passArray8ToWasm0(memo, wasm.__wbindgen_export);
            var len1 = WASM_VECTOR_LEN;
            var ptr2 = isLikeNone(label) ? 0 : passStringToWasm0(label, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            var len2 = WASM_VECTOR_LEN;
            var ptr3 = isLikeNone(message) ? 0 : passStringToWasm0(message, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            var len3 = WASM_VECTOR_LEN;
            wasm.paymentrequest_new(retptr, ptr0, len0, amount, ptr1, len1, ptr2, len2, ptr3, len3, addHeapObject(other_params));
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            this.__wbg_ptr = r0;
            PaymentRequestFinalization.register(this, this.__wbg_ptr, this);
            return this;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @returns {any}
     */
    other_params() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.paymentrequest_other_params(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return takeObject(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @returns {string}
     */
    recipient_address() {
        let deferred1_0;
        let deferred1_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.paymentrequest_recipient_address(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            deferred1_0 = r0;
            deferred1_1 = r1;
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export4(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @param {string} recipient_address
     * @param {bigint} amount
     * @returns {PaymentRequest}
     */
    static simple_payment(recipient_address, amount) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(recipient_address, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            wasm.paymentrequest_simple_payment(retptr, ptr0, len0, amount);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return PaymentRequest.__wrap(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
}
if (Symbol.dispose) PaymentRequest.prototype[Symbol.dispose] = PaymentRequest.prototype.free;

export class Pczt {
    static __wrap(ptr) {
        const obj = Object.create(Pczt.prototype);
        obj.__wbg_ptr = ptr;
        PcztFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        PcztFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_pczt_free(ptr, 0);
    }
    /**
     * Combines this PCZT with another representing the same transaction.
     *
     * Used after external signing: `self` is the full PCZT kept by the wallet
     * (with witnesses) and `other` is the redacted PCZT returned by the signer
     * (with signatures). The result carries both, ready for proving and
     * extraction.
     * @param {Pczt} other
     * @returns {Pczt}
     */
    combine(other) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            _assertClass(other, Pczt);
            wasm.pczt_combine(retptr, this.__wbg_ptr, other.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return Pczt.__wrap(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Deserialize to a Pczt from postcard bytes.
     * @param {Uint8Array} bytes
     * @returns {Pczt}
     */
    static from_bytes(bytes) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passArray8ToWasm0(bytes, wasm.__wbindgen_export);
            const len0 = WASM_VECTOR_LEN;
            wasm.pczt_from_bytes(retptr, ptr0, len0);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return Pczt.__wrap(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Returns a Pczt from a JSON object
     * @param {any} s
     * @returns {Pczt}
     */
    static from_json(s) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.pczt_from_json(retptr, addHeapObject(s));
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return Pczt.__wrap(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Strips prover-only data so the PCZT is small enough to transport to an
     * external signer (e.g. Keystone) as an animated QR.
     *
     * Mirrors the "redact for signer" role used by Zashi: removes spend
     * witnesses (the per-note Merkle authentication paths that dominate the
     * serialized size) and the `zcash_client_backend` proprietary metadata.
     * The signer only needs the spend randomizers, values and derivation
     * paths, so the redacted PCZT still produces valid signatures. The
     * original (full) PCZT must be retained and merged back with [`combine`]
     * after signing to restore the witnesses required for proving.
     *
     * [`combine`]: Pczt::combine
     * @returns {Pczt}
     */
    redact_for_signer() {
        const ret = wasm.pczt_redact_for_signer(this.__wbg_ptr);
        return Pczt.__wrap(ret);
    }
    /**
     * Returns the postcard serialization of the Pczt.
     * @returns {Uint8Array}
     */
    serialize() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.pczt_serialize(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            if (r3) {
                throw takeObject(r2);
            }
            var v1 = getArrayU8FromWasm0(r0, r1).slice();
            wasm.__wbindgen_export4(r0, r1 * 1, 1);
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Returns a JSON object with the details of the Pczt.
     * @returns {any}
     */
    to_json() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.pczt_to_json(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return takeObject(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
}
if (Symbol.dispose) Pczt.prototype[Symbol.dispose] = Pczt.prototype.free;

export class ProofGenerationKey {
    static __wrap(ptr) {
        const obj = Object.create(ProofGenerationKey.prototype);
        obj.__wbg_ptr = ptr;
        ProofGenerationKeyFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        ProofGenerationKeyFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_proofgenerationkey_free(ptr, 0);
    }
}
if (Symbol.dispose) ProofGenerationKey.prototype[Symbol.dispose] = ProofGenerationKey.prototype.free;

export class Proposal {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        ProposalFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_proposal_free(ptr, 0);
    }
}
if (Symbol.dispose) Proposal.prototype[Symbol.dispose] = Proposal.prototype.free;

export class SeedFingerprint {
    static __wrap(ptr) {
        const obj = Object.create(SeedFingerprint.prototype);
        obj.__wbg_ptr = ptr;
        SeedFingerprintFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        SeedFingerprintFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_seedfingerprint_free(ptr, 0);
    }
    /**
     * @param {Uint8Array} bytes
     * @returns {SeedFingerprint}
     */
    static from_bytes(bytes) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passArray8ToWasm0(bytes, wasm.__wbindgen_export);
            const len0 = WASM_VECTOR_LEN;
            wasm.seedfingerprint_from_bytes(retptr, ptr0, len0);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return SeedFingerprint.__wrap(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @param {Uint8Array} seed
     */
    constructor(seed) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passArray8ToWasm0(seed, wasm.__wbindgen_export);
            const len0 = WASM_VECTOR_LEN;
            wasm.seedfingerprint_new(retptr, ptr0, len0);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            this.__wbg_ptr = r0;
            SeedFingerprintFinalization.register(this, this.__wbg_ptr, this);
            return this;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @returns {Uint8Array}
     */
    to_bytes() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.seedfingerprint_to_bytes(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var v1 = getArrayU8FromWasm0(r0, r1).slice();
            wasm.__wbindgen_export4(r0, r1 * 1, 1);
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
}
if (Symbol.dispose) SeedFingerprint.prototype[Symbol.dispose] = SeedFingerprint.prototype.free;

export class TransactionRequest {
    static __wrap(ptr) {
        const obj = Object.create(TransactionRequest.prototype);
        obj.__wbg_ptr = ptr;
        TransactionRequestFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        TransactionRequestFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_transactionrequest_free(ptr, 0);
    }
    /**
     * @returns {TransactionRequest}
     */
    static empty() {
        const ret = wasm.transactionrequest_empty();
        return TransactionRequest.__wrap(ret);
    }
    /**
     * @param {string} uri
     * @returns {TransactionRequest}
     */
    static from_uri(uri) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(uri, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            wasm.transactionrequest_from_uri(retptr, ptr0, len0);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return TransactionRequest.__wrap(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @param {PaymentRequest[]} payments
     */
    constructor(payments) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passArrayJsValueToWasm0(payments, wasm.__wbindgen_export);
            const len0 = WASM_VECTOR_LEN;
            wasm.transactionrequest_new(retptr, ptr0, len0);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            this.__wbg_ptr = r0;
            TransactionRequestFinalization.register(this, this.__wbg_ptr, this);
            return this;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @returns {PaymentRequest[]}
     */
    payment_requests() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.transactionrequest_payment_requests(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var v1 = getArrayJsValueFromWasm0(r0, r1).slice();
            wasm.__wbindgen_export4(r0, r1 * 4, 4);
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @returns {string}
     */
    to_uri() {
        let deferred1_0;
        let deferred1_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.transactionrequest_to_uri(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            deferred1_0 = r0;
            deferred1_1 = r1;
            return getStringFromWasm0(r0, r1);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export4(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {bigint}
     */
    total() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.transactionrequest_total(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getBigInt64(retptr + 8 * 0, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            if (r3) {
                throw takeObject(r2);
            }
            return BigInt.asUintN(64, r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
}
if (Symbol.dispose) TransactionRequest.prototype[Symbol.dispose] = TransactionRequest.prototype.free;

/**
 * A Zcash viewing key
 *
 * This is a wrapper around the `zcash_keys::keys::ViewingKey` type.
 * UFVKs should be generated from a spending key by calling `to_unified_full_viewing_key`
 * They can also be encoded and decoded to a canonical string representation
 */
export class UnifiedFullViewingKey {
    static __wrap(ptr) {
        const obj = Object.create(UnifiedFullViewingKey.prototype);
        obj.__wbg_ptr = ptr;
        UnifiedFullViewingKeyFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        UnifiedFullViewingKeyFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_unifiedfullviewingkey_free(ptr, 0);
    }
    /**
     * @param {string} network
     * @returns {string}
     */
    encode(network) {
        let deferred3_0;
        let deferred3_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(network, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            wasm.unifiedfullviewingkey_encode(retptr, this.__wbg_ptr, ptr0, len0);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr2 = r0;
            var len2 = r1;
            if (r3) {
                ptr2 = 0; len2 = 0;
                throw takeObject(r2);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export4(deferred3_0, deferred3_1, 1);
        }
    }
    /**
     * @param {string} network
     * @param {string} encoding
     */
    constructor(network, encoding) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(network, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(encoding, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            wasm.unifiedfullviewingkey_new(retptr, ptr0, len0, ptr1, len1);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            this.__wbg_ptr = r0;
            UnifiedFullViewingKeyFinalization.register(this, this.__wbg_ptr, this);
            return this;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
}
if (Symbol.dispose) UnifiedFullViewingKey.prototype[Symbol.dispose] = UnifiedFullViewingKey.prototype.free;

export class UnifiedSpendingKey {
    static __wrap(ptr) {
        const obj = Object.create(UnifiedSpendingKey.prototype);
        obj.__wbg_ptr = ptr;
        UnifiedSpendingKeyFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        UnifiedSpendingKeyFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_unifiedspendingkey_free(ptr, 0);
    }
    /**
     * Construct a new UnifiedSpendingKey
     *
     * # Arguments
     *
     * * `network` - Must be either "main" or "test"
     * * `seed` - At least 32 bytes of entry. Care should be taken as to how this is derived
     * * `hd_index` - [ZIP32](https://zips.z.cash/zip-0032) hierarchical deterministic index of the account
     * @param {string} network
     * @param {Uint8Array} seed
     * @param {number} hd_index
     */
    constructor(network, seed, hd_index) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(network, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passArray8ToWasm0(seed, wasm.__wbindgen_export);
            const len1 = WASM_VECTOR_LEN;
            wasm.unifiedspendingkey_new(retptr, ptr0, len0, ptr1, len1, hd_index);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            this.__wbg_ptr = r0;
            UnifiedSpendingKeyFinalization.register(this, this.__wbg_ptr, this);
            return this;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @returns {ProofGenerationKey}
     */
    to_sapling_proof_generation_key() {
        const ret = wasm.unifiedspendingkey_to_sapling_proof_generation_key(this.__wbg_ptr);
        return ProofGenerationKey.__wrap(ret);
    }
    /**
     * Obtain the UFVK corresponding to this spending key
     * @returns {UnifiedFullViewingKey}
     */
    to_unified_full_viewing_key() {
        const ret = wasm.unifiedspendingkey_to_unified_full_viewing_key(this.__wbg_ptr);
        return UnifiedFullViewingKey.__wrap(ret);
    }
}
if (Symbol.dispose) UnifiedSpendingKey.prototype[Symbol.dispose] = UnifiedSpendingKey.prototype.free;

export class WalletSummary {
    static __wrap(ptr) {
        const obj = Object.create(WalletSummary.prototype);
        obj.__wbg_ptr = ptr;
        WalletSummaryFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    toJSON() {
        return {
            chain_tip_height: this.chain_tip_height,
            fully_scanned_height: this.fully_scanned_height,
            next_ironwood_subtree_index: this.next_ironwood_subtree_index,
            next_orchard_subtree_index: this.next_orchard_subtree_index,
            next_sapling_subtree_index: this.next_sapling_subtree_index,
            account_balances: this.account_balances,
        };
    }
    toString() {
        return JSON.stringify(this);
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WalletSummaryFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_walletsummary_free(ptr, 0);
    }
    /**
     * @returns {number}
     */
    get chain_tip_height() {
        const ret = wasm.__wbg_get_walletsummary_chain_tip_height(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    get fully_scanned_height() {
        const ret = wasm.__wbg_get_walletsummary_fully_scanned_height(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {bigint}
     */
    get next_ironwood_subtree_index() {
        const ret = wasm.__wbg_get_walletsummary_next_ironwood_subtree_index(this.__wbg_ptr);
        return BigInt.asUintN(64, ret);
    }
    /**
     * @returns {bigint}
     */
    get next_orchard_subtree_index() {
        const ret = wasm.__wbg_get_walletsummary_next_orchard_subtree_index(this.__wbg_ptr);
        return BigInt.asUintN(64, ret);
    }
    /**
     * @returns {bigint}
     */
    get next_sapling_subtree_index() {
        const ret = wasm.__wbg_get_walletsummary_next_sapling_subtree_index(this.__wbg_ptr);
        return BigInt.asUintN(64, ret);
    }
    /**
     * @param {number} arg0
     */
    set chain_tip_height(arg0) {
        wasm.__wbg_set_walletsummary_chain_tip_height(this.__wbg_ptr, arg0);
    }
    /**
     * @param {number} arg0
     */
    set fully_scanned_height(arg0) {
        wasm.__wbg_set_walletsummary_fully_scanned_height(this.__wbg_ptr, arg0);
    }
    /**
     * @param {bigint} arg0
     */
    set next_ironwood_subtree_index(arg0) {
        wasm.__wbg_set_walletsummary_next_ironwood_subtree_index(this.__wbg_ptr, arg0);
    }
    /**
     * @param {bigint} arg0
     */
    set next_orchard_subtree_index(arg0) {
        wasm.__wbg_set_walletsummary_next_orchard_subtree_index(this.__wbg_ptr, arg0);
    }
    /**
     * @param {bigint} arg0
     */
    set next_sapling_subtree_index(arg0) {
        wasm.__wbg_set_walletsummary_next_sapling_subtree_index(this.__wbg_ptr, arg0);
    }
    /**
     * @returns {any}
     */
    get account_balances() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.walletsummary_account_balances(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return takeObject(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
}
if (Symbol.dispose) WalletSummary.prototype[Symbol.dispose] = WalletSummary.prototype.free;

export class WebWallet {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WebWalletFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_webwallet_free(ptr, 0);
    }
    /**
     * Benchmark helper: download a block range over gRPC-web without scanning.
     * Returns the number of blocks fetched. Used to probe download throughput.
     * @param {number} start_height
     * @param {number} count
     * @returns {Promise<number>}
     */
    bench_download_range(start_height, count) {
        const ret = wasm.webwallet_bench_download_range(this.__wbg_ptr, start_height, count);
        return takeObject(ret);
    }
    /**
     * Benchmark helper: one baseline journal batch with per-stage timings (ms)
     * as JSON `{"subtree","setup","download","scan"}`, or undefined when synced.
     * When `refresh` is false, session setup is skipped (call
     * `sync_session_setup` once first).
     * @param {number | null | undefined} batch_size
     * @param {boolean} refresh
     * @returns {Promise<string | undefined>}
     */
    bench_prepare_sync_batch_timed(batch_size, refresh) {
        const ret = wasm.webwallet_bench_prepare_sync_batch_timed(this.__wbg_ptr, isLikeNone(batch_size) ? Number.MAX_SAFE_INTEGER : (batch_size) >>> 0, refresh);
        return takeObject(ret);
    }
    /**
     * @param {string} account_name
     * @param {string} seed_phrase
     * @param {number} account_hd_index
     * @param {number | null} [birthday_height]
     * @param {string | null} [passphrase]
     * @returns {Promise<number>}
     */
    create_account(account_name, seed_phrase, account_hd_index, birthday_height, passphrase) {
        const ptr0 = passStringToWasm0(account_name, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(seed_phrase, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        var ptr2 = isLikeNone(passphrase) ? 0 : passStringToWasm0(passphrase, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len2 = WASM_VECTOR_LEN;
        const ret = wasm.webwallet_create_account(this.__wbg_ptr, ptr0, len0, ptr1, len1, account_hd_index, isLikeNone(birthday_height) ? Number.MAX_SAFE_INTEGER : (birthday_height) >>> 0, ptr2, len2);
        return takeObject(ret);
    }
    /**
     * Import a watch-only spending account from an encoded UFVK (hardware wallet flow).
     *
     * # Arguments
     * * `account_name` - Display name for the account
     * * `ufvk` - The encoded Unified Full Viewing Key exported by the device
     * * `seed_fingerprint` - 32-byte ZIP-32 seed fingerprint exported by the device
     * * `account_hd_index` - ZIP-32 account index on the device
     * * `birthday_height` - Block height to start scanning from
     * * `key_source` - Optional tag describing the key origin (e.g. "keystone")
     *
     * # Returns
     * The new account id
     * @param {string} account_name
     * @param {string} ufvk
     * @param {Uint8Array} seed_fingerprint
     * @param {number} account_hd_index
     * @param {number | null} [birthday_height]
     * @param {string | null} [key_source]
     * @returns {Promise<number>}
     */
    create_account_ufvk(account_name, ufvk, seed_fingerprint, account_hd_index, birthday_height, key_source) {
        const ptr0 = passStringToWasm0(account_name, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(ufvk, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passArray8ToWasm0(seed_fingerprint, wasm.__wbindgen_export);
        const len2 = WASM_VECTOR_LEN;
        var ptr3 = isLikeNone(key_source) ? 0 : passStringToWasm0(key_source, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        var len3 = WASM_VECTOR_LEN;
        const ret = wasm.webwallet_create_account_ufvk(this.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2, account_hd_index, isLikeNone(birthday_height) ? Number.MAX_SAFE_INTEGER : (birthday_height) >>> 0, ptr3, len3);
        return takeObject(ret);
    }
    /**
     * @returns {Promise<Uint8Array>}
     */
    db_to_bytes() {
        const ret = wasm.webwallet_db_to_bytes(this.__wbg_ptr);
        return takeObject(ret);
    }
    /**
     * @param {Function} callback
     * @returns {Promise<void>}
     */
    db_to_bytes_chunked(callback) {
        const ret = wasm.webwallet_db_to_bytes_chunked(this.__wbg_ptr, addHeapObject(callback));
        return takeObject(ret);
    }
    /**
     * @param {number} account_id
     * @param {string} to_address
     * @param {Uint8Array | null | undefined} memo
     * @param {bigint} marginal_fee_zats
     * @returns {Promise<bigint>}
     */
    estimate_orchard_migration_fee(account_id, to_address, memo, marginal_fee_zats) {
        const ptr0 = passStringToWasm0(to_address, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(memo) ? 0 : passArray8ToWasm0(memo, wasm.__wbindgen_export);
        var len1 = WASM_VECTOR_LEN;
        const ret = wasm.webwallet_estimate_orchard_migration_fee(this.__wbg_ptr, account_id, ptr0, len0, ptr1, len1, marginal_fee_zats);
        return takeObject(ret);
    }
    /**
     * @param {number} account_id
     * @param {string} to_address
     * @param {Uint8Array | null | undefined} memo
     * @param {bigint} marginal_fee_zats
     * @returns {Promise<any>}
     */
    estimate_pczt_max_transfer(account_id, to_address, memo, marginal_fee_zats) {
        const ptr0 = passStringToWasm0(to_address, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(memo) ? 0 : passArray8ToWasm0(memo, wasm.__wbindgen_export);
        var len1 = WASM_VECTOR_LEN;
        const ret = wasm.webwallet_estimate_pczt_max_transfer(this.__wbg_ptr, account_id, ptr0, len0, ptr1, len1, marginal_fee_zats);
        return takeObject(ret);
    }
    /**
     * @param {number} account_id
     * @param {string} to_address
     * @param {Uint8Array | null | undefined} memo
     * @param {bigint} marginal_fee_zats
     * @returns {Promise<any>}
     */
    estimate_pczt_max_transparent_transfer(account_id, to_address, memo, marginal_fee_zats) {
        const ptr0 = passStringToWasm0(to_address, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(memo) ? 0 : passArray8ToWasm0(memo, wasm.__wbindgen_export);
        var len1 = WASM_VECTOR_LEN;
        const ret = wasm.webwallet_estimate_pczt_max_transparent_transfer(this.__wbg_ptr, account_id, ptr0, len0, ptr1, len1, marginal_fee_zats);
        return takeObject(ret);
    }
    /**
     * @param {number} account_id
     * @param {bigint} marginal_fee_zats
     * @returns {Promise<bigint>}
     */
    estimate_pczt_shield_fee(account_id, marginal_fee_zats) {
        const ret = wasm.webwallet_estimate_pczt_shield_fee(this.__wbg_ptr, account_id, marginal_fee_zats);
        return takeObject(ret);
    }
    /**
     * @param {number} account_id
     * @param {string} to_address
     * @param {bigint} value
     * @param {Uint8Array | null | undefined} memo
     * @param {bigint} marginal_fee_zats
     * @returns {Promise<bigint>}
     */
    estimate_pczt_transfer_fee(account_id, to_address, value, memo, marginal_fee_zats) {
        const ptr0 = passStringToWasm0(to_address, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(memo) ? 0 : passArray8ToWasm0(memo, wasm.__wbindgen_export);
        var len1 = WASM_VECTOR_LEN;
        const ret = wasm.webwallet_estimate_pczt_transfer_fee(this.__wbg_ptr, account_id, ptr0, len0, value, ptr1, len1, marginal_fee_zats);
        return takeObject(ret);
    }
    /**
     * @param {number} account_id
     * @param {string} to_address
     * @param {bigint} value
     * @param {Uint8Array | null | undefined} memo
     * @param {bigint} marginal_fee_zats
     * @returns {Promise<bigint>}
     */
    estimate_pczt_transparent_transfer_fee(account_id, to_address, value, memo, marginal_fee_zats) {
        const ptr0 = passStringToWasm0(to_address, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(memo) ? 0 : passArray8ToWasm0(memo, wasm.__wbindgen_export);
        var len1 = WASM_VECTOR_LEN;
        const ret = wasm.webwallet_estimate_pczt_transparent_transfer_fee(this.__wbg_ptr, account_id, ptr0, len0, value, ptr1, len1, marginal_fee_zats);
        return takeObject(ret);
    }
    /**
     * @param {number} account_id
     * @param {string} to_address
     * @param {bigint} value
     * @param {"shielded" | "transparent"} funding_source
     * @param {Uint8Array | null} [memo]
     * @param {bigint | null} [marginal_fee_zats]
     * @param {number | null} [expiry_delta_blocks]
     * @returns {TransferEstimate}
     */
    estimate_transfer(account_id, to_address, value, funding_source, memo, marginal_fee_zats, expiry_delta_blocks) {
        const ptr0 = passStringToWasm0(to_address, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(funding_source, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        var ptr2 = isLikeNone(memo) ? 0 : passArray8ToWasm0(memo, wasm.__wbindgen_export);
        var len2 = WASM_VECTOR_LEN;
        const ret = wasm.webwallet_estimate_transfer(this.__wbg_ptr, account_id, ptr0, len0, value, ptr1, len1, ptr2, len2, !isLikeNone(marginal_fee_zats), isLikeNone(marginal_fee_zats) ? BigInt(0) : marginal_fee_zats, isLikeNone(expiry_delta_blocks) ? Number.MAX_SAFE_INTEGER : (expiry_delta_blocks) >>> 0);
        return takeObject(ret);
    }
    /**
     * Get the birthday height for a specific account
     *
     * # Arguments
     *
     * * `account_id` - The account ID to get the birthday height for
     *
     * # Returns
     *
     * The birthday height (block height) for the specified account
     *
     * # Example
     *
     * ```javascript
     * const accountId = 0;
     * const birthdayHeight = await wallet.get_account_birthday(accountId);
     * console.log(`Account ${accountId} birthday height: ${birthdayHeight}`);
     * ```
     * @param {number} account_id
     * @returns {Promise<number>}
     */
    get_account_birthday(account_id) {
        const ret = wasm.webwallet_get_account_birthday(this.__wbg_ptr, account_id);
        return takeObject(ret);
    }
    /**
     * Get the HD account index for an account.
     *
     * This returns the ZIP-32 account index that was used to derive this account's keys.
     * Returns None if the account was imported and doesn't have derivation info.
     *
     * # Arguments
     *
     * * `account_id` - The account ID
     *
     * # Returns
     *
     * The HD account index (u32), or None if not available
     * @param {number} account_id
     * @returns {Promise<number | undefined>}
     */
    get_account_hd_index(account_id) {
        const ret = wasm.webwallet_get_account_hd_index(this.__wbg_ptr, account_id);
        return takeObject(ret);
    }
    /**
     * Get account ID by account name
     *
     * # Arguments
     *
     * * `account_name` - The name of the account to find
     *
     * # Returns
     *
     * Returns the account ID as u32 if found
     *
     * # Example
     *
     * ```javascript
     * const accountId = await wallet.get_account_id_by_name("MyAccount");
     * console.log("Account ID:", accountId);
     * ```
     * @param {string} account_name
     * @returns {Promise<number>}
     */
    get_account_id_by_name(account_name) {
        const ptr0 = passStringToWasm0(account_name, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webwallet_get_account_id_by_name(this.__wbg_ptr, ptr0, len0);
        return takeObject(ret);
    }
    /**
     * Find the block height closest to a given Unix timestamp
     *
     * Uses binary search to efficiently locate the block height corresponding
     * to a specific time. Useful for determining wallet birthday heights based
     * on dates.
     *
     * # Arguments
     *
     * * `time` - Unix timestamp (seconds since epoch)
     *
     * # Returns
     *
     * Block height closest to the given timestamp (within ~1000 blocks precision)
     *
     * # Example
     *
     * ```javascript
     * // Find block height for January 1, 2024
     * const timestamp = Math.floor(new Date('2024-01-01').getTime() / 1000);
     * const height = await wallet.get_block_by_time(timestamp);
     * console.log(`Block height at 2024-01-01: ${height}`);
     *
     * // Use as birthday height when importing a wallet
     * const birthdayTimestamp = Math.floor(new Date('2023-06-15').getTime() / 1000);
     * const birthdayHeight = await wallet.get_block_by_time(birthdayTimestamp);
     * ```
     * @param {number} time
     * @returns {Promise<number>}
     */
    get_block_by_time(time) {
        const ret = wasm.webwallet_get_block_by_time(this.__wbg_ptr, time);
        return takeObject(ret);
    }
    /**
     * Get the Unix timestamp of a block at a given height
     *
     * # Arguments
     *
     * * `height` - The block height
     *
     * # Returns
     *
     * Unix timestamp (seconds since epoch) when the block was mined
     *
     * # Example
     *
     * ```javascript
     * const height = 2855000;
     * const timestamp = await wallet.get_block_time(height);
     * const date = new Date(timestamp * 1000);
     * console.log(`Block ${height} was mined at: ${date}`);
     * ```
     * @param {number} height
     * @returns {Promise<number>}
     */
    get_block_time(height) {
        const ret = wasm.webwallet_get_block_time(this.__wbg_ptr, height);
        return takeObject(ret);
    }
    /**
     * Returns the configured NU6.3 activation height for the selected network.
     * @returns {number | undefined}
     */
    get_ironwood_activation_height() {
        const ret = wasm.webwallet_get_ironwood_activation_height(this.__wbg_ptr);
        return ret === Number.MAX_SAFE_INTEGER ? undefined : ret;
    }
    /**
     * @returns {Promise<bigint>}
     */
    get_latest_block() {
        const ret = wasm.webwallet_get_latest_block(this.__wbg_ptr);
        return takeObject(ret);
    }
    /**
     * Get the latest block height from the blockchain
     *
     * # Returns
     *
     * The current chain tip height
     *
     * # Example
     *
     * ```javascript
     * const latestHeight = await wallet.get_latest_height();
     * console.log(`Current blockchain height: ${latestHeight}`);
     * ```
     * @returns {Promise<number>}
     */
    get_latest_height() {
        const ret = wasm.webwallet_get_latest_height(this.__wbg_ptr);
        return takeObject(ret);
    }
    /**
     * Generate and persist the next shielded receiving address for an account.
     *
     * This returns a unified address containing the account's available shielded receivers
     * (Orchard/Sapling) and no transparent receiver. It advances the wallet's stored address
     * position, so callers should persist the wallet DB after using this method.
     *
     * # Arguments
     *
     * * `account_id` - The account ID to generate the next shielded address for
     *
     * # Returns
     *
     * Returns a UnifiedAddresses object. `transparent` and `address_index` will be null.
     * @param {number} account_id
     * @returns {Promise<any>}
     */
    get_next_shielded_address(account_id) {
        const ret = wasm.webwallet_get_next_shielded_address(this.__wbg_ptr, account_id);
        return takeObject(ret);
    }
    /**
     * @param {number} account_id
     * @returns {Promise<any>}
     */
    get_transaction_history(account_id) {
        const ret = wasm.webwallet_get_transaction_history(this.__wbg_ptr, account_id);
        return takeObject(ret);
    }
    /**
     * Get addresses for an account
     *
     * # Arguments
     *
     * * `account_id` - The account ID to get addresses for
     *
     * # Returns
     *
     * Returns a UnifiedAddresses object containing:
     * - unified: The shielded unified address (u1 format with Orchard/Sapling receivers only)
     * - orchard: Orchard-only unified address (if available)
     * - sapling: Sapling-only unified address (u1 format, if available)
     * - sapling_zs: Sapling traditional address (zs format, if available)
     * - transparent: Transparent P2PKH address (if available)
     *
     * # Example
     *
     * ```javascript
     * const addresses = await wallet.get_unified_addresses(accountId);
     * console.log("Unified Address:", addresses.unified);
     * console.log("Sapling (zs):", addresses.sapling_zs);
     * console.log("Transparent:", addresses.transparent);
     * ```
     * @param {number} account_id
     * @returns {Promise<any>}
     */
    get_unified_addresses(account_id) {
        const ret = wasm.webwallet_get_unified_addresses(this.__wbg_ptr, account_id);
        return takeObject(ret);
    }
    /**
     * @returns {Promise<WalletSummary | undefined>}
     */
    get_wallet_summary() {
        const ret = wasm.webwallet_get_wallet_summary(this.__wbg_ptr);
        return takeObject(ret);
    }
    /**
     * @param {string} network
     * @param {string} lightwalletd_url
     * @param {Uint8Array | null} [db_bytes]
     */
    constructor(network, lightwalletd_url, db_bytes) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(network, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            const ptr1 = passStringToWasm0(lightwalletd_url, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            var ptr2 = isLikeNone(db_bytes) ? 0 : passArray8ToWasm0(db_bytes, wasm.__wbindgen_export);
            var len2 = WASM_VECTOR_LEN;
            wasm.webwallet_new(retptr, ptr0, len0, ptr1, len1, ptr2, len2);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            this.__wbg_ptr = r0;
            WebWalletFinalization.register(this, this.__wbg_ptr, this);
            return this;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @param {number} account_id
     * @param {string} to_address
     * @param {UnifiedSpendingKey} usk
     * @param {SeedFingerprint} seed_fp
     * @param {ProofGenerationKey | null | undefined} sapling_proof_gen_key
     * @param {Uint8Array | null | undefined} memo
     * @param {bigint} marginal_fee_zats
     * @param {number | null} [expiry_delta_blocks]
     * @returns {Promise<string>}
     */
    oneclick_pczt_orchard_migration(account_id, to_address, usk, seed_fp, sapling_proof_gen_key, memo, marginal_fee_zats, expiry_delta_blocks) {
        const ptr0 = passStringToWasm0(to_address, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        _assertClass(usk, UnifiedSpendingKey);
        var ptr1 = usk.__destroy_into_raw();
        _assertClass(seed_fp, SeedFingerprint);
        var ptr2 = seed_fp.__destroy_into_raw();
        let ptr3 = 0;
        if (!isLikeNone(sapling_proof_gen_key)) {
            _assertClass(sapling_proof_gen_key, ProofGenerationKey);
            ptr3 = sapling_proof_gen_key.__destroy_into_raw();
        }
        var ptr4 = isLikeNone(memo) ? 0 : passArray8ToWasm0(memo, wasm.__wbindgen_export);
        var len4 = WASM_VECTOR_LEN;
        const ret = wasm.webwallet_oneclick_pczt_orchard_migration(this.__wbg_ptr, account_id, ptr0, len0, ptr1, ptr2, ptr3, ptr4, len4, marginal_fee_zats, isLikeNone(expiry_delta_blocks) ? Number.MAX_SAFE_INTEGER : (expiry_delta_blocks) >>> 0);
        return takeObject(ret);
    }
    /**
     * @param {number} account_id
     * @param {UnifiedSpendingKey} usk
     * @param {SeedFingerprint} seed_fp
     * @param {ProofGenerationKey | null} [sapling_proof_gen_key]
     * @returns {Promise<string>}
     */
    oneclick_pczt_shield(account_id, usk, seed_fp, sapling_proof_gen_key) {
        _assertClass(usk, UnifiedSpendingKey);
        var ptr0 = usk.__destroy_into_raw();
        _assertClass(seed_fp, SeedFingerprint);
        var ptr1 = seed_fp.__destroy_into_raw();
        let ptr2 = 0;
        if (!isLikeNone(sapling_proof_gen_key)) {
            _assertClass(sapling_proof_gen_key, ProofGenerationKey);
            ptr2 = sapling_proof_gen_key.__destroy_into_raw();
        }
        const ret = wasm.webwallet_oneclick_pczt_shield(this.__wbg_ptr, account_id, ptr0, ptr1, ptr2);
        return takeObject(ret);
    }
    /**
     * @param {number} account_id
     * @param {UnifiedSpendingKey} usk
     * @param {SeedFingerprint} seed_fp
     * @param {ProofGenerationKey | null | undefined} sapling_proof_gen_key
     * @param {bigint} marginal_fee_zats
     * @param {number | null} [expiry_delta_blocks]
     * @returns {Promise<string>}
     */
    oneclick_pczt_shield_dynamic_fee(account_id, usk, seed_fp, sapling_proof_gen_key, marginal_fee_zats, expiry_delta_blocks) {
        _assertClass(usk, UnifiedSpendingKey);
        var ptr0 = usk.__destroy_into_raw();
        _assertClass(seed_fp, SeedFingerprint);
        var ptr1 = seed_fp.__destroy_into_raw();
        let ptr2 = 0;
        if (!isLikeNone(sapling_proof_gen_key)) {
            _assertClass(sapling_proof_gen_key, ProofGenerationKey);
            ptr2 = sapling_proof_gen_key.__destroy_into_raw();
        }
        const ret = wasm.webwallet_oneclick_pczt_shield_dynamic_fee(this.__wbg_ptr, account_id, ptr0, ptr1, ptr2, marginal_fee_zats, isLikeNone(expiry_delta_blocks) ? Number.MAX_SAFE_INTEGER : (expiry_delta_blocks) >>> 0);
        return takeObject(ret);
    }
    /**
     * @param {number} account_id
     * @param {string} to_address
     * @param {bigint} value
     * @param {UnifiedSpendingKey} usk
     * @param {SeedFingerprint} seed_fp
     * @param {ProofGenerationKey | null} [sapling_proof_gen_key]
     * @param {Uint8Array | null} [memo]
     * @returns {Promise<string>}
     */
    oneclick_pczt_transfer(account_id, to_address, value, usk, seed_fp, sapling_proof_gen_key, memo) {
        const ptr0 = passStringToWasm0(to_address, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        _assertClass(usk, UnifiedSpendingKey);
        var ptr1 = usk.__destroy_into_raw();
        _assertClass(seed_fp, SeedFingerprint);
        var ptr2 = seed_fp.__destroy_into_raw();
        let ptr3 = 0;
        if (!isLikeNone(sapling_proof_gen_key)) {
            _assertClass(sapling_proof_gen_key, ProofGenerationKey);
            ptr3 = sapling_proof_gen_key.__destroy_into_raw();
        }
        var ptr4 = isLikeNone(memo) ? 0 : passArray8ToWasm0(memo, wasm.__wbindgen_export);
        var len4 = WASM_VECTOR_LEN;
        const ret = wasm.webwallet_oneclick_pczt_transfer(this.__wbg_ptr, account_id, ptr0, len0, value, ptr1, ptr2, ptr3, ptr4, len4);
        return takeObject(ret);
    }
    /**
     * @param {number} account_id
     * @param {string} to_address
     * @param {bigint} value
     * @param {UnifiedSpendingKey} usk
     * @param {SeedFingerprint} seed_fp
     * @param {ProofGenerationKey | null | undefined} sapling_proof_gen_key
     * @param {Uint8Array | null | undefined} memo
     * @param {bigint} marginal_fee_zats
     * @param {number | null} [expiry_delta_blocks]
     * @returns {Promise<string>}
     */
    oneclick_pczt_transfer_dynamic_fee(account_id, to_address, value, usk, seed_fp, sapling_proof_gen_key, memo, marginal_fee_zats, expiry_delta_blocks) {
        const ptr0 = passStringToWasm0(to_address, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        _assertClass(usk, UnifiedSpendingKey);
        var ptr1 = usk.__destroy_into_raw();
        _assertClass(seed_fp, SeedFingerprint);
        var ptr2 = seed_fp.__destroy_into_raw();
        let ptr3 = 0;
        if (!isLikeNone(sapling_proof_gen_key)) {
            _assertClass(sapling_proof_gen_key, ProofGenerationKey);
            ptr3 = sapling_proof_gen_key.__destroy_into_raw();
        }
        var ptr4 = isLikeNone(memo) ? 0 : passArray8ToWasm0(memo, wasm.__wbindgen_export);
        var len4 = WASM_VECTOR_LEN;
        const ret = wasm.webwallet_oneclick_pczt_transfer_dynamic_fee(this.__wbg_ptr, account_id, ptr0, len0, value, ptr1, ptr2, ptr3, ptr4, len4, marginal_fee_zats, isLikeNone(expiry_delta_blocks) ? Number.MAX_SAFE_INTEGER : (expiry_delta_blocks) >>> 0);
        return takeObject(ret);
    }
    /**
     * @param {number} account_id
     * @param {string} to_address
     * @param {bigint} value
     * @param {UnifiedSpendingKey} usk
     * @param {SeedFingerprint} seed_fp
     * @param {ProofGenerationKey | null} [sapling_proof_gen_key]
     * @param {Uint8Array | null} [memo]
     * @returns {Promise<string>}
     */
    oneclick_pczt_transparent_transfer(account_id, to_address, value, usk, seed_fp, sapling_proof_gen_key, memo) {
        const ptr0 = passStringToWasm0(to_address, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        _assertClass(usk, UnifiedSpendingKey);
        var ptr1 = usk.__destroy_into_raw();
        _assertClass(seed_fp, SeedFingerprint);
        var ptr2 = seed_fp.__destroy_into_raw();
        let ptr3 = 0;
        if (!isLikeNone(sapling_proof_gen_key)) {
            _assertClass(sapling_proof_gen_key, ProofGenerationKey);
            ptr3 = sapling_proof_gen_key.__destroy_into_raw();
        }
        var ptr4 = isLikeNone(memo) ? 0 : passArray8ToWasm0(memo, wasm.__wbindgen_export);
        var len4 = WASM_VECTOR_LEN;
        const ret = wasm.webwallet_oneclick_pczt_transparent_transfer(this.__wbg_ptr, account_id, ptr0, len0, value, ptr1, ptr2, ptr3, ptr4, len4);
        return takeObject(ret);
    }
    /**
     * @param {number} account_id
     * @param {string} to_address
     * @param {bigint} value
     * @param {UnifiedSpendingKey} usk
     * @param {SeedFingerprint} seed_fp
     * @param {ProofGenerationKey | null | undefined} sapling_proof_gen_key
     * @param {Uint8Array | null | undefined} memo
     * @param {bigint} marginal_fee_zats
     * @param {number | null} [expiry_delta_blocks]
     * @returns {Promise<string>}
     */
    oneclick_pczt_transparent_transfer_dynamic_fee(account_id, to_address, value, usk, seed_fp, sapling_proof_gen_key, memo, marginal_fee_zats, expiry_delta_blocks) {
        const ptr0 = passStringToWasm0(to_address, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        _assertClass(usk, UnifiedSpendingKey);
        var ptr1 = usk.__destroy_into_raw();
        _assertClass(seed_fp, SeedFingerprint);
        var ptr2 = seed_fp.__destroy_into_raw();
        let ptr3 = 0;
        if (!isLikeNone(sapling_proof_gen_key)) {
            _assertClass(sapling_proof_gen_key, ProofGenerationKey);
            ptr3 = sapling_proof_gen_key.__destroy_into_raw();
        }
        var ptr4 = isLikeNone(memo) ? 0 : passArray8ToWasm0(memo, wasm.__wbindgen_export);
        var len4 = WASM_VECTOR_LEN;
        const ret = wasm.webwallet_oneclick_pczt_transparent_transfer_dynamic_fee(this.__wbg_ptr, account_id, ptr0, len0, value, ptr1, ptr2, ptr3, ptr4, len4, marginal_fee_zats, isLikeNone(expiry_delta_blocks) ? Number.MAX_SAFE_INTEGER : (expiry_delta_blocks) >>> 0);
        return takeObject(ret);
    }
    /**
     * @param {number} account_id
     * @param {string} to_address
     * @param {bigint} value
     * @param {UnifiedSpendingKey} usk
     * @param {"shielded" | "transparent"} funding_source
     * @param {Uint8Array | null} [memo]
     * @param {bigint | null} [marginal_fee_zats]
     * @param {number | null} [expiry_delta_blocks]
     * @returns {TransferResult}
     */
    oneclick_transfer(account_id, to_address, value, usk, funding_source, memo, marginal_fee_zats, expiry_delta_blocks) {
        const ptr0 = passStringToWasm0(to_address, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        _assertClass(usk, UnifiedSpendingKey);
        var ptr1 = usk.__destroy_into_raw();
        const ptr2 = passStringToWasm0(funding_source, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        var ptr3 = isLikeNone(memo) ? 0 : passArray8ToWasm0(memo, wasm.__wbindgen_export);
        var len3 = WASM_VECTOR_LEN;
        const ret = wasm.webwallet_oneclick_transfer(this.__wbg_ptr, account_id, ptr0, len0, value, ptr1, ptr2, len2, ptr3, len3, !isLikeNone(marginal_fee_zats), isLikeNone(marginal_fee_zats) ? BigInt(0) : marginal_fee_zats, isLikeNone(expiry_delta_blocks) ? Number.MAX_SAFE_INTEGER : (expiry_delta_blocks) >>> 0);
        return takeObject(ret);
    }
    /**
     * Create an unsigned, unproven PCZT for a transfer (hardware wallet flow).
     *
     * The returned PCZT carries ZIP-32 derivation metadata so an external signer
     * (e.g. Keystone) can locate its keys. Serialize it with `Pczt.serialize()`
     * before transporting via QR. `marginal_fee_zats` selects the ZIP-317 fee
     * tier so hardware sends honor the same dynamic-fee tiers as software sends.
     * @param {number} account_id
     * @param {string} to_address
     * @param {bigint} value
     * @param {Uint8Array | null | undefined} memo
     * @param {bigint} marginal_fee_zats
     * @returns {Promise<Pczt>}
     */
    pczt_create(account_id, to_address, value, memo, marginal_fee_zats) {
        const ptr0 = passStringToWasm0(to_address, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(memo) ? 0 : passArray8ToWasm0(memo, wasm.__wbindgen_export);
        var len1 = WASM_VECTOR_LEN;
        const ret = wasm.webwallet_pczt_create(this.__wbg_ptr, account_id, ptr0, len0, value, ptr1, len1, marginal_fee_zats);
        return takeObject(ret);
    }
    /**
     * Create a V6 PCZT that sweeps spendable Orchard funds to this account's Ironwood pool.
     * @param {number} account_id
     * @param {string} to_address
     * @param {Uint8Array | null | undefined} memo
     * @param {bigint} marginal_fee_zats
     * @param {number | null} [expiry_delta_blocks]
     * @returns {Promise<Pczt>}
     */
    pczt_orchard_migration_create(account_id, to_address, memo, marginal_fee_zats, expiry_delta_blocks) {
        const ptr0 = passStringToWasm0(to_address, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(memo) ? 0 : passArray8ToWasm0(memo, wasm.__wbindgen_export);
        var len1 = WASM_VECTOR_LEN;
        const ret = wasm.webwallet_pczt_orchard_migration_create(this.__wbg_ptr, account_id, ptr0, len0, ptr1, len1, marginal_fee_zats, isLikeNone(expiry_delta_blocks) ? Number.MAX_SAFE_INTEGER : (expiry_delta_blocks) >>> 0);
        return takeObject(ret);
    }
    /**
     * Prove and broadcast an externally signed PCZT (hardware wallet flow).
     *
     * Hardware-wallet accounts never hold Sapling notes (Keystone supports
     * transparent + Orchard only), so no Sapling proof generation key is needed.
     *
     * # Returns
     * Transaction ID string on success
     * @param {Pczt} pczt
     * @returns {Promise<string>}
     */
    pczt_prove_and_send(pczt) {
        _assertClass(pczt, Pczt);
        var ptr0 = pczt.__destroy_into_raw();
        const ret = wasm.webwallet_pczt_prove_and_send(this.__wbg_ptr, ptr0);
        return takeObject(ret);
    }
    /**
     * Create an unsigned, unproven shielding PCZT (transparent -> Orchard) for
     * external signing (hardware wallet flow). `marginal_fee_zats` selects the
     * ZIP-317 fee tier so hardware shields honor the dynamic-fee tiers too.
     * @param {number} account_id
     * @param {bigint} marginal_fee_zats
     * @returns {Promise<Pczt>}
     */
    pczt_shield_create(account_id, marginal_fee_zats) {
        const ret = wasm.webwallet_pczt_shield_create(this.__wbg_ptr, account_id, marginal_fee_zats);
        return takeObject(ret);
    }
    /**
     * Journal-aware sync: download + scan one batch, return journal data for persistence.
     * Returns null if fully synced, or Uint8Array with journal entry data.
     * When `refresh` is false, session setup is skipped (call `sync_session_setup`
     * once at the start of the catch-up session first).
     * @param {number | null | undefined} batch_size
     * @param {boolean} refresh
     * @returns {Promise<Uint8Array | undefined>}
     */
    prepare_sync_batch(batch_size, refresh) {
        const ret = wasm.webwallet_prepare_sync_batch(this.__wbg_ptr, isLikeNone(batch_size) ? Number.MAX_SAFE_INTEGER : (batch_size) >>> 0, refresh);
        return takeObject(ret);
    }
    /**
     * Optimized journal-aware sync: same as [`prepare_sync_batch`] but uses
     * External-only IVK scanning for contiguous forward ranges.
     * @param {number | null} [batch_size]
     * @returns {Promise<Uint8Array | undefined>}
     */
    prepare_sync_batch_optimized(batch_size) {
        const ret = wasm.webwallet_prepare_sync_batch_optimized(this.__wbg_ptr, isLikeNone(batch_size) ? Number.MAX_SAFE_INTEGER : (batch_size) >>> 0);
        return takeObject(ret);
    }
    /**
     * Replay multiple journal entries in sequence (for recovery).
     * @param {Uint8Array[]} entries
     * @returns {Promise<void>}
     */
    replay_journal_entries(entries) {
        const ptr0 = passArrayJsValueToWasm0(entries, wasm.__wbindgen_export);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webwallet_replay_journal_entries(this.__wbg_ptr, ptr0, len0);
        return takeObject(ret);
    }
    /**
     * Scan a journal entry for recovery (replay from base snapshot + journal).
     * Returns true if more scan ranges remain.
     * @param {Uint8Array} journal_data
     * @returns {Promise<boolean>}
     */
    scan_journal_entry(journal_data) {
        const ptr0 = passArray8ToWasm0(journal_data, wasm.__wbindgen_export);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webwallet_scan_journal_entry(this.__wbg_ptr, ptr0, len0);
        return takeObject(ret);
    }
    /**
     * @param {Uint8Array} txids
     * @returns {Promise<void>}
     */
    send_authorized_transactions(txids) {
        const ptr0 = passArray8ToWasm0(txids, wasm.__wbindgen_export);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webwallet_send_authorized_transactions(this.__wbg_ptr, ptr0, len0);
        return takeObject(ret);
    }
    /**
     * @returns {Promise<void>}
     */
    sync() {
        const ret = wasm.webwallet_sync(this.__wbg_ptr);
        return takeObject(ret);
    }
    /**
     * Runs a single batch of the sync process without looping.
     * Returns `true` if a scan range was processed, `false` if no scan ranges were suggested.
     * @param {number | null} [batch_size]
     * @returns {Promise<boolean>}
     */
    sync_next_batch(batch_size) {
        const ret = wasm.webwallet_sync_next_batch(this.__wbg_ptr, isLikeNone(batch_size) ? Number.MAX_SAFE_INTEGER : (batch_size) >>> 0);
        return takeObject(ret);
    }
    /**
     * Runs a single batch using the optimized scanning strategy.
     * Returns `true` if a scan range was processed, `false` if no scan ranges were suggested.
     * @param {number | null} [batch_size]
     * @returns {Promise<boolean>}
     */
    sync_next_batch_optimized(batch_size) {
        const ret = wasm.webwallet_sync_next_batch_optimized(this.__wbg_ptr, isLikeNone(batch_size) ? Number.MAX_SAFE_INTEGER : (batch_size) >>> 0);
        return takeObject(ret);
    }
    /**
     * Run session-level setup (subtree roots, chain tip, transparent UTXOs)
     * once before a multi-batch catch-up so subsequent batches can skip it.
     * @returns {Promise<void>}
     */
    sync_session_setup() {
        const ret = wasm.webwallet_sync_session_setup(this.__wbg_ptr);
        return takeObject(ret);
    }
}
if (Symbol.dispose) WebWallet.prototype[Symbol.dispose] = WebWallet.prototype.free;

/**
 * Generate a new 24-word BIP-39 mnemonic phrase.
 *
 * # Arguments
 *
 * * `word_count` - Number of words in the mnemonic. Must be 24 for new wallet generation.
 *
 * # Returns
 *
 * A string containing the mnemonic phrase
 *
 * # Example
 *
 * ```javascript
 * const mnemonic = generate_mnemonic(24);
 * console.log(mnemonic); // "abandon abandon abandon ... art"
 * ```
 * @param {number} word_count
 * @returns {string}
 */
export function generate_mnemonic(word_count) {
    let deferred2_0;
    let deferred2_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.generate_mnemonic(retptr, word_count);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        var ptr1 = r0;
        var len1 = r1;
        if (r3) {
            ptr1 = 0; len1 = 0;
            throw takeObject(r2);
        }
        deferred2_0 = ptr1;
        deferred2_1 = len1;
        return getStringFromWasm0(ptr1, len1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export4(deferred2_0, deferred2_1, 1);
    }
}

/**
 * @param {number} num_threads
 * @returns {Promise<any>}
 */
export function initThreadPool(num_threads) {
    const ret = wasm.initThreadPool(num_threads);
    return takeObject(ret);
}

/**
 * Get all addresses (shielded unified, orchard, sapling, transparent) from a mnemonic phrase.
 *
 * # Arguments
 *
 * * `network` - Network name ("main" or "test")
 * * `phrase` - The mnemonic phrase
 * * `passphrase` - Optional passphrase (use empty string "" if none)
 * * `account_index` - The account index for HD derivation (usually 0)
 *
 * # Returns
 *
 * A JSON object containing all address types
 *
 * # Example
 *
 * ```javascript
 * const mnemonic = generate_mnemonic(24);
 * const addresses = mnemonic_to_addresses(
 *     "main",
 *     mnemonic,
 *     "",  // no passphrase
 *     0    // first account
 * );
 * console.log("Unified:", addresses.unified);
 * console.log("Orchard:", addresses.orchard);
 * console.log("Sapling:", addresses.sapling);
 * console.log("Transparent:", addresses.transparent);
 * ```
 * @param {string} network
 * @param {string} phrase
 * @param {string} passphrase
 * @param {number} account_index
 * @returns {any}
 */
export function mnemonic_to_addresses(network, phrase, passphrase, account_index) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(network, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(phrase, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(passphrase, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        wasm.mnemonic_to_addresses(retptr, ptr0, len0, ptr1, len1, ptr2, len2, account_index);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return takeObject(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Convert a mnemonic phrase to a seed with an optional passphrase.
 *
 * # Arguments
 *
 * * `phrase` - The mnemonic phrase
 * * `passphrase` - Optional passphrase (use empty string for no passphrase)
 *
 * # Returns
 *
 * A 64-byte seed that can be used for key derivation
 *
 * # Example
 *
 * ```javascript
 * const seed = mnemonic_to_seed("abandon abandon abandon ... art", "");
 * console.log(seed); // Uint8Array of 64 bytes
 * ```
 * @param {string} phrase
 * @param {string} passphrase
 * @returns {Uint8Array}
 */
export function mnemonic_to_seed(phrase, passphrase) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(phrase, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(passphrase, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        wasm.mnemonic_to_seed(retptr, ptr0, len0, ptr1, len1);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        if (r3) {
            throw takeObject(r2);
        }
        var v3 = getArrayU8FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export4(r0, r1 * 1, 1);
        return v3;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Generate a SeedFingerprint from a mnemonic phrase.
 *
 * # Arguments
 *
 * * `phrase` - The mnemonic phrase
 * * `passphrase` - Optional passphrase (use empty string "" if none)
 *
 * # Returns
 *
 * A SeedFingerprint that can be used for key derivation path identification
 *
 * # Example
 *
 * ```javascript
 * const mnemonic = generate_mnemonic(24);
 * const seed_fp = mnemonic_to_seed_fingerprint(
 *     mnemonic,
 *     ""  // no passphrase
 * );
 * ```
 * @param {string} phrase
 * @param {string} passphrase
 * @returns {SeedFingerprint}
 */
export function mnemonic_to_seed_fingerprint(phrase, passphrase) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(phrase, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(passphrase, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        wasm.mnemonic_to_seed_fingerprint(retptr, ptr0, len0, ptr1, len1);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return SeedFingerprint.__wrap(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Generate a UnifiedSpendingKey from a mnemonic phrase.
 *
 * # Arguments
 *
 * * `network` - Network name ("main" or "test")
 * * `phrase` - The mnemonic phrase
 * * `passphrase` - Optional passphrase (use empty string "" if none)
 * * `account_index` - The account index for HD derivation (usually 0)
 *
 * # Returns
 *
 * A UnifiedSpendingKey that can be used for signing transactions
 *
 * # Example
 *
 * ```javascript
 * const mnemonic = generate_mnemonic(24);
 * const usk = mnemonic_to_spending_key(
 *     "main",
 *     mnemonic,
 *     "",  // no passphrase
 *     0    // first account
 * );
 * ```
 * @param {string} network
 * @param {string} phrase
 * @param {string} passphrase
 * @param {number} account_index
 * @returns {UnifiedSpendingKey}
 */
export function mnemonic_to_spending_key(network, phrase, passphrase, account_index) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(network, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(phrase, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(passphrase, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        wasm.mnemonic_to_spending_key(retptr, ptr0, len0, ptr1, len1, ptr2, len2, account_index);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return UnifiedSpendingKey.__wrap(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Get a transparent address and public key from a mnemonic at a specific external address index.
 *
 * # Arguments
 *
 * * `network` - Network name ("main" or "test")
 * * `phrase` - The mnemonic phrase
 * * `passphrase` - Optional passphrase (use empty string "" if none)
 * * `account_index` - The account index for HD derivation (usually 0)
 * * `address_index` - The external transparent child address index
 * @param {string} network
 * @param {string} phrase
 * @param {string} passphrase
 * @param {number} account_index
 * @param {number} address_index
 * @returns {any}
 */
export function mnemonic_to_transparent_address(network, phrase, passphrase, account_index, address_index) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(network, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(phrase, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(passphrase, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        wasm.mnemonic_to_transparent_address(retptr, ptr0, len0, ptr1, len1, ptr2, len2, account_index, address_index);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return takeObject(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Parse a unified address string and extract individual receiver addresses.
 *
 * # Arguments
 *
 * * `network` - Network name ("main" or "test")
 * * `unified_address` - The unified address string to parse
 *
 * # Returns
 *
 * A JSON object containing individual address types extracted from the unified address
 *
 * # Example
 *
 * ```javascript
 * const addresses = parse_unified_address(
 *     "main",
 *     "u1..." // unified address string
 * );
 * console.log("Orchard:", addresses.orchard);
 * console.log("Sapling:", addresses.sapling);
 * console.log("Transparent:", addresses.transparent);
 * ```
 * @param {string} network
 * @param {string} unified_address
 * @returns {any}
 */
export function parse_unified_address(network, unified_address) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(network, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(unified_address, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        wasm.parse_unified_address(retptr, ptr0, len0, ptr1, len1);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return takeObject(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Sign a message with the transparent address private key
 *
 * This implements the Bitcoin/Zcash message signing convention compatible with
 * the `signmessage` RPC call.
 *
 * # Arguments
 *
 * * `network` - Network type ("main" or "test")
 * * `usk` - The Unified Spending Key
 * * `message` - The message to sign
 * * `address_index` - Optional external transparent address index to sign with. If omitted, the default full unified address index is used.
 *
 * # Returns
 *
 * Returns a JSON object with:
 * - signature: Hex-encoded signature (65 bytes: recovery_id + r + s)
 * - pubkey: Hex-encoded uncompressed public key (65 bytes: 04 + x + y)
 * - address: The transparent address used for signing
 *
 * # Example
 *
 * ```javascript
 * const result = await sign_message_with_transparent("main", usk, "Hello Zcash!", undefined);
 * console.log("Signature:", result.signature);  // hex format
 * console.log("Public Key:", result.pubkey);     // uncompressed hex (65 bytes)
 * console.log("Address:", result.address);       // t-address
 * ```
 * @param {string} network
 * @param {UnifiedSpendingKey} usk
 * @param {string} message
 * @param {number | null} [address_index]
 * @returns {any}
 */
export function sign_message_with_transparent(network, usk, message, address_index) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(network, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        _assertClass(usk, UnifiedSpendingKey);
        var ptr1 = usk.__destroy_into_raw();
        const ptr2 = passStringToWasm0(message, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        wasm.sign_message_with_transparent(retptr, ptr0, len0, ptr1, ptr2, len2, isLikeNone(address_index) ? Number.MAX_SAFE_INTEGER : (address_index) >>> 0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return takeObject(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

export function start() {
    wasm.start();
}

/**
 * Validate a BIP-39 mnemonic phrase.
 *
 * # Arguments
 *
 * * `phrase` - The mnemonic phrase to validate
 *
 * # Returns
 *
 * Returns Ok(()) if valid, or an error if invalid
 *
 * # Example
 *
 * ```javascript
 * try {
 *     validate_mnemonic("abandon abandon abandon ... art");
 *     console.log("Valid mnemonic");
 * } catch (e) {
 *     console.error("Invalid mnemonic:", e);
 * }
 * ```
 * @param {string} phrase
 */
export function validate_mnemonic(phrase) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(phrase, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.validate_mnemonic(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        if (r1) {
            throw takeObject(r0);
        }
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

export class wbg_rayon_PoolBuilder {
    static __wrap(ptr) {
        const obj = Object.create(wbg_rayon_PoolBuilder.prototype);
        obj.__wbg_ptr = ptr;
        wbg_rayon_PoolBuilderFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        wbg_rayon_PoolBuilderFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wbg_rayon_poolbuilder_free(ptr, 0);
    }
    build() {
        wasm.wbg_rayon_poolbuilder_build(this.__wbg_ptr);
    }
    /**
     * @returns {number}
     */
    numThreads() {
        const ret = wasm.wbg_rayon_poolbuilder_numThreads(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {number}
     */
    receiver() {
        const ret = wasm.wbg_rayon_poolbuilder_receiver(this.__wbg_ptr);
        return ret >>> 0;
    }
}
if (Symbol.dispose) wbg_rayon_PoolBuilder.prototype[Symbol.dispose] = wbg_rayon_PoolBuilder.prototype.free;

/**
 * @param {number} receiver
 */
export function wbg_rayon_start_worker(receiver) {
    wasm.wbg_rayon_start_worker(receiver);
}
function __wbg_get_imports(memory) {
    const import0 = {
        __proto__: null,
        __wbg_Error_ef53bc310eb298a0: function(arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return addHeapObject(ret);
        },
        __wbg_Number_6b506e6536831eaa: function(arg0) {
            const ret = Number(getObject(arg0));
            return ret;
        },
        __wbg___wbindgen_boolean_get_1a45e2c38d4d41b9: function(arg0) {
            const v = getObject(arg0);
            const ret = typeof(v) === 'boolean' ? v : undefined;
            return isLikeNone(ret) ? 0xFFFFFF : ret ? 1 : 0;
        },
        __wbg___wbindgen_debug_string_0accd80f45e5faa2: function(arg0, arg1) {
            const ret = debugString(getObject(arg1));
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_is_function_754e9f305ff6029e: function(arg0) {
            const ret = typeof(getObject(arg0)) === 'function';
            return ret;
        },
        __wbg___wbindgen_is_object_56732c2bc353f41d: function(arg0) {
            const val = getObject(arg0);
            const ret = typeof(val) === 'object' && val !== null;
            return ret;
        },
        __wbg___wbindgen_is_string_c236cabd84a4d769: function(arg0) {
            const ret = typeof(getObject(arg0)) === 'string';
            return ret;
        },
        __wbg___wbindgen_is_undefined_67b456be8673d3d7: function(arg0) {
            const ret = getObject(arg0) === undefined;
            return ret;
        },
        __wbg___wbindgen_jsval_loose_eq_2c56564c75129511: function(arg0, arg1) {
            const ret = getObject(arg0) == getObject(arg1);
            return ret;
        },
        __wbg___wbindgen_memory_fbc4c3e30b409f08: function() {
            const ret = wasm.memory;
            return addHeapObject(ret);
        },
        __wbg___wbindgen_module_5dcc25d553a4424f: function() {
            const ret = wasmModule;
            return addHeapObject(ret);
        },
        __wbg___wbindgen_number_get_9bb1761122181af2: function(arg0, arg1) {
            const obj = getObject(arg1);
            const ret = typeof(obj) === 'number' ? obj : undefined;
            getDataViewMemory0().setFloat64(arg0 + 8 * 1, isLikeNone(ret) ? 0 : ret, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, !isLikeNone(ret), true);
        },
        __wbg___wbindgen_rethrow_c4d99b4b53265290: function(arg0) {
            throw takeObject(arg0);
        },
        __wbg___wbindgen_string_get_72bdf95d3ae505b1: function(arg0, arg1) {
            const obj = getObject(arg1);
            const ret = typeof(obj) === 'string' ? obj : undefined;
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_throw_1506f2235d1bdba0: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg__wbg_cb_unref_61db23ac97f16c31: function(arg0) {
            getObject(arg0)._wbg_cb_unref();
        },
        __wbg_abort_2ec46222bf378517: function(arg0) {
            getObject(arg0).abort();
        },
        __wbg_abort_b29d719932441c95: function(arg0, arg1) {
            getObject(arg0).abort(getObject(arg1));
        },
        __wbg_append_e1746995edcb0170: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
            getObject(arg0).append(getStringFromWasm0(arg1, arg2), getStringFromWasm0(arg3, arg4));
        }, arguments); },
        __wbg_async_ed0edf9269e8f04a: function(arg0) {
            const ret = getObject(arg0).async;
            return ret;
        },
        __wbg_body_61a0827da5b6d2bc: function(arg0) {
            const ret = getObject(arg0).body;
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        },
        __wbg_buffer_a1f116eb4fdb1531: function(arg0) {
            const ret = getObject(arg0).buffer;
            return addHeapObject(ret);
        },
        __wbg_buffer_d370c8cae5692933: function(arg0) {
            const ret = getObject(arg0).buffer;
            return addHeapObject(ret);
        },
        __wbg_byobRequest_2c89fb4ab478fa09: function(arg0) {
            const ret = getObject(arg0).byobRequest;
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        },
        __wbg_byteLength_2c6dc3b4b85d3547: function(arg0) {
            const ret = getObject(arg0).byteLength;
            return ret;
        },
        __wbg_byteOffset_349aa9bf0a183eca: function(arg0) {
            const ret = getObject(arg0).byteOffset;
            return ret;
        },
        __wbg_call_8a89609d89f6608a: function() { return handleError(function (arg0, arg1) {
            const ret = getObject(arg0).call(getObject(arg1));
            return addHeapObject(ret);
        }, arguments); },
        __wbg_call_9c758de292015997: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = getObject(arg0).call(getObject(arg1), getObject(arg2));
            return addHeapObject(ret);
        }, arguments); },
        __wbg_cancel_3dedc1c2245a59d4: function(arg0) {
            const ret = getObject(arg0).cancel();
            return addHeapObject(ret);
        },
        __wbg_catch_17ae9c6dfb88ad8a: function(arg0, arg1) {
            const ret = getObject(arg0).catch(getObject(arg1));
            return addHeapObject(ret);
        },
        __wbg_clearTimeout_4ce8abcdd5386a8f: function(arg0) {
            const ret = clearTimeout(takeObject(arg0));
            return addHeapObject(ret);
        },
        __wbg_close_4859304bbf0f8208: function() { return handleError(function (arg0) {
            getObject(arg0).close();
        }, arguments); },
        __wbg_close_6f12196fe155e8d2: function() { return handleError(function (arg0) {
            getObject(arg0).close();
        }, arguments); },
        __wbg_create_8d32ae11bf54585f: function(arg0) {
            const ret = Object.create(getObject(arg0));
            return addHeapObject(ret);
        },
        __wbg_crypto_48300657fced39f9: function(arg0) {
            const ret = getObject(arg0).crypto;
            return addHeapObject(ret);
        },
        __wbg_data_93740e25a9d5b212: function(arg0) {
            const ret = getObject(arg0).data;
            return addHeapObject(ret);
        },
        __wbg_debug_6d96d354ecb8cdb3: function(arg0, arg1, arg2, arg3) {
            console.debug(getObject(arg0), getObject(arg1), getObject(arg2), getObject(arg3));
        },
        __wbg_debug_78b457f1effb3792: function(arg0) {
            console.debug(getObject(arg0));
        },
        __wbg_done_60cf307fcc680536: function(arg0) {
            const ret = getObject(arg0).done;
            return ret;
        },
        __wbg_enqueue_09035479e2081625: function() { return handleError(function (arg0, arg1) {
            getObject(arg0).enqueue(getObject(arg1));
        }, arguments); },
        __wbg_error_78ff5b3a29b770e0: function(arg0) {
            console.error(getObject(arg0));
        },
        __wbg_error_9ad1450feb5d541d: function(arg0, arg1, arg2, arg3) {
            console.error(getObject(arg0), getObject(arg1), getObject(arg2), getObject(arg3));
        },
        __wbg_error_a6fa202b58aa1cd3: function(arg0, arg1) {
            let deferred0_0;
            let deferred0_1;
            try {
                deferred0_0 = arg0;
                deferred0_1 = arg1;
                console.error(getStringFromWasm0(arg0, arg1));
            } finally {
                wasm.__wbindgen_export4(deferred0_0, deferred0_1, 1);
            }
        },
        __wbg_fetch_9cd66815bfb42f0c: function(arg0, arg1, arg2) {
            const ret = getObject(arg0).fetch(getObject(arg1), getObject(arg2));
            return addHeapObject(ret);
        },
        __wbg_fetch_b9ec7cc04e1ed641: function(arg0, arg1) {
            const ret = fetch(getObject(arg0), getObject(arg1));
            return addHeapObject(ret);
        },
        __wbg_getRandomValues_263d0aa5464054ee: function() { return handleError(function (arg0, arg1) {
            getObject(arg0).getRandomValues(getObject(arg1));
        }, arguments); },
        __wbg_getReader_b4b1868fbca77dbe: function() { return handleError(function (arg0) {
            const ret = getObject(arg0).getReader();
            return addHeapObject(ret);
        }, arguments); },
        __wbg_getTime_00b3f7db575e4ef5: function(arg0) {
            const ret = getObject(arg0).getTime();
            return ret;
        },
        __wbg_get_1f8f054ddbaa7db2: function() { return handleError(function (arg0, arg1) {
            const ret = Reflect.get(getObject(arg0), getObject(arg1));
            return addHeapObject(ret);
        }, arguments); },
        __wbg_get_2b48c7d0d006a781: function(arg0, arg1) {
            const ret = getObject(arg0)[arg1 >>> 0];
            return addHeapObject(ret);
        },
        __wbg_get_done_ea9eb315d4ec1e81: function(arg0) {
            const ret = getObject(arg0).done;
            return isLikeNone(ret) ? 0xFFFFFF : ret ? 1 : 0;
        },
        __wbg_get_unchecked_33f6e5c9e2f2d6b2: function(arg0, arg1) {
            const ret = getObject(arg0)[arg1 >>> 0];
            return addHeapObject(ret);
        },
        __wbg_get_value_c68fe2e1a76c69ca: function(arg0) {
            const ret = getObject(arg0).value;
            return addHeapObject(ret);
        },
        __wbg_has_73740b27f436fed3: function() { return handleError(function (arg0, arg1) {
            const ret = Reflect.has(getObject(arg0), getObject(arg1));
            return ret;
        }, arguments); },
        __wbg_headers_0feb63d2d374b44a: function(arg0) {
            const ret = getObject(arg0).headers;
            return addHeapObject(ret);
        },
        __wbg_info_5cfb3f6c22c53cf9: function(arg0, arg1, arg2, arg3) {
            console.info(getObject(arg0), getObject(arg1), getObject(arg2), getObject(arg3));
        },
        __wbg_info_af7f45292ba9b0ea: function(arg0) {
            console.info(getObject(arg0));
        },
        __wbg_instanceof_ArrayBuffer_8f49811467741499: function(arg0) {
            let result;
            try {
                result = getObject(arg0) instanceof ArrayBuffer;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_Uint8Array_86f30649f63ef9c2: function(arg0) {
            let result;
            try {
                result = getObject(arg0) instanceof Uint8Array;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_Window_e093be59ee9a8e14: function(arg0) {
            let result;
            try {
                result = getObject(arg0) instanceof Window;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_isArray_67c2c9c4313f4448: function(arg0) {
            const ret = Array.isArray(getObject(arg0));
            return ret;
        },
        __wbg_isSafeInteger_66acec27e09e99a7: function(arg0) {
            const ret = Number.isSafeInteger(getObject(arg0));
            return ret;
        },
        __wbg_iterator_8732428d309e270e: function() {
            const ret = Symbol.iterator;
            return addHeapObject(ret);
        },
        __wbg_length_4a591ecaa01354d9: function(arg0) {
            const ret = getObject(arg0).length;
            return ret;
        },
        __wbg_length_66f1a4b2e9026940: function(arg0) {
            const ret = getObject(arg0).length;
            return ret;
        },
        __wbg_mark_310e9a031ccc84ff: function() { return handleError(function (arg0, arg1, arg2) {
            getObject(arg0).mark(getStringFromWasm0(arg1, arg2));
        }, arguments); },
        __wbg_mark_742962d66a858cc5: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            getObject(arg0).mark(getStringFromWasm0(arg1, arg2), getObject(arg3));
        }, arguments); },
        __wbg_measure_54743279e77e77bb: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4, arg5, arg6) {
            getObject(arg0).measure(getStringFromWasm0(arg1, arg2), getStringFromWasm0(arg3, arg4), getStringFromWasm0(arg5, arg6));
        }, arguments); },
        __wbg_measure_cd1f7667d5f5517d: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            getObject(arg0).measure(getStringFromWasm0(arg1, arg2), getObject(arg3));
        }, arguments); },
        __wbg_msCrypto_8c6d45a75ef1d3da: function(arg0) {
            const ret = getObject(arg0).msCrypto;
            return addHeapObject(ret);
        },
        __wbg_new_0_445c13a750296eb6: function() {
            const ret = new Date();
            return addHeapObject(ret);
        },
        __wbg_new_0d09705104e164af: function() { return handleError(function () {
            const ret = new AbortController();
            return addHeapObject(ret);
        }, arguments); },
        __wbg_new_227d7c05414eb861: function() {
            const ret = new Error();
            return addHeapObject(ret);
        },
        __wbg_new_50bb5ebeecef71a8: function(arg0, arg1) {
            const ret = new Error(getStringFromWasm0(arg0, arg1));
            return addHeapObject(ret);
        },
        __wbg_new_578aeef4b6b94378: function(arg0) {
            const ret = new Uint8Array(getObject(arg0));
            return addHeapObject(ret);
        },
        __wbg_new_b682b81e8eaaf027: function(arg0, arg1) {
            try {
                var state0 = {a: arg0, b: arg1};
                var cb0 = (arg0, arg1) => {
                    const a = state0.a;
                    state0.a = 0;
                    try {
                        return __wasm_bindgen_func_elem_8986(a, state0.b, arg0, arg1);
                    } finally {
                        state0.a = a;
                    }
                };
                const ret = new Promise(cb0);
                return addHeapObject(ret);
            } finally {
                state0.a = 0;
            }
        },
        __wbg_new_ce1ab61c1c2b300d: function() {
            const ret = new Object();
            return addHeapObject(ret);
        },
        __wbg_new_d90091b82fdf5b91: function() {
            const ret = new Array();
            return addHeapObject(ret);
        },
        __wbg_new_dc32d91df76232c8: function(arg0) {
            const ret = new Int32Array(getObject(arg0));
            return addHeapObject(ret);
        },
        __wbg_new_e436d06bc8e77460: function() { return handleError(function () {
            const ret = new Headers();
            return addHeapObject(ret);
        }, arguments); },
        __wbg_new_from_slice_18fa1f71286d66b8: function(arg0, arg1) {
            const ret = new Uint8Array(getArrayU8FromWasm0(arg0, arg1));
            return addHeapObject(ret);
        },
        __wbg_new_typed_bf31d18f92484486: function(arg0, arg1) {
            try {
                var state0 = {a: arg0, b: arg1};
                var cb0 = (arg0, arg1) => {
                    const a = state0.a;
                    state0.a = 0;
                    try {
                        return __wasm_bindgen_func_elem_8986(a, state0.b, arg0, arg1);
                    } finally {
                        state0.a = a;
                    }
                };
                const ret = new Promise(cb0);
                return addHeapObject(ret);
            } finally {
                state0.a = 0;
            }
        },
        __wbg_new_with_byte_offset_and_length_d836f26d916dd9ad: function(arg0, arg1, arg2) {
            const ret = new Uint8Array(getObject(arg0), arg1 >>> 0, arg2 >>> 0);
            return addHeapObject(ret);
        },
        __wbg_new_with_length_36a4998e27b014c5: function(arg0) {
            const ret = new Uint8Array(arg0 >>> 0);
            return addHeapObject(ret);
        },
        __wbg_new_with_str_and_init_bcd02b79a793d27f: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = new Request(getStringFromWasm0(arg0, arg1), getObject(arg2));
            return addHeapObject(ret);
        }, arguments); },
        __wbg_new_worker_227309bcfae51cd3: function(arg0, arg1) {
            const ret = new Worker(getStringFromWasm0(arg0, arg1));
            return addHeapObject(ret);
        },
        __wbg_next_9e03acdf51c4960d: function(arg0) {
            const ret = getObject(arg0).next;
            return addHeapObject(ret);
        },
        __wbg_next_eb8ca7351fa27906: function() { return handleError(function (arg0) {
            const ret = getObject(arg0).next();
            return addHeapObject(ret);
        }, arguments); },
        __wbg_node_95beb7570492fd97: function(arg0) {
            const ret = getObject(arg0).node;
            return addHeapObject(ret);
        },
        __wbg_now_190933fa139cc119: function() {
            const ret = Date.now();
            return ret;
        },
        __wbg_of_5d9c1c77975668d1: function(arg0, arg1, arg2) {
            const ret = Array.of(getObject(arg0), getObject(arg1), getObject(arg2));
            return addHeapObject(ret);
        },
        __wbg_paymentrequest_new: function(arg0) {
            const ret = PaymentRequest.__wrap(arg0);
            return addHeapObject(ret);
        },
        __wbg_paymentrequest_unwrap: function(arg0) {
            const ret = PaymentRequest.__unwrap(getObject(arg0));
            return ret;
        },
        __wbg_pczt_new: function(arg0) {
            const ret = Pczt.__wrap(arg0);
            return addHeapObject(ret);
        },
        __wbg_performance_757310249566272b: function() {
            const ret = globalThis.performance;
            return addHeapObject(ret);
        },
        __wbg_postMessage_c28ba544836193c8: function() { return handleError(function (arg0, arg1) {
            getObject(arg0).postMessage(getObject(arg1));
        }, arguments); },
        __wbg_process_b2fea42461d03994: function(arg0) {
            const ret = getObject(arg0).process;
            return addHeapObject(ret);
        },
        __wbg_prototypesetcall_3249fc62a0fafa30: function(arg0, arg1, arg2) {
            Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), getObject(arg2));
        },
        __wbg_queueMicrotask_35c611f4a14830b2: function(arg0) {
            queueMicrotask(getObject(arg0));
        },
        __wbg_queueMicrotask_404ed0a58e0b63cc: function(arg0) {
            const ret = getObject(arg0).queueMicrotask;
            return addHeapObject(ret);
        },
        __wbg_randomFillSync_ca9f178fb14c88cb: function() { return handleError(function (arg0, arg1) {
            getObject(arg0).randomFillSync(takeObject(arg1));
        }, arguments); },
        __wbg_read_282e152a24fd0856: function(arg0) {
            const ret = getObject(arg0).read();
            return addHeapObject(ret);
        },
        __wbg_releaseLock_cd76770b7f82a961: function(arg0) {
            getObject(arg0).releaseLock();
        },
        __wbg_require_7a9419e39d796c95: function() { return handleError(function () {
            const ret = module.require;
            return addHeapObject(ret);
        }, arguments); },
        __wbg_resolve_25a7e548d5881dca: function(arg0) {
            const ret = Promise.resolve(getObject(arg0));
            return addHeapObject(ret);
        },
        __wbg_respond_33b6f330b6d299fd: function() { return handleError(function (arg0, arg1) {
            getObject(arg0).respond(arg1 >>> 0);
        }, arguments); },
        __wbg_setTimeout_3f4119f26240bd79: function(arg0, arg1) {
            const ret = setTimeout(getObject(arg0), arg1);
            return addHeapObject(ret);
        },
        __wbg_set_25ef40a9aeff260d: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
            getObject(arg0).set(getStringFromWasm0(arg1, arg2), getStringFromWasm0(arg3, arg4));
        }, arguments); },
        __wbg_set_29c99a8aac1c01e5: function(arg0, arg1, arg2) {
            getObject(arg0).set(getArrayU8FromWasm0(arg1, arg2));
        },
        __wbg_set_6be42768c690e380: function(arg0, arg1, arg2) {
            getObject(arg0)[takeObject(arg1)] = takeObject(arg2);
        },
        __wbg_set_6e30c9374c26414c: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = Reflect.set(getObject(arg0), getObject(arg1), getObject(arg2));
            return ret;
        }, arguments); },
        __wbg_set_body_36614c7e61546809: function(arg0, arg1) {
            getObject(arg0).body = getObject(arg1);
        },
        __wbg_set_cache_488ea16c11cbf20d: function(arg0, arg1) {
            getObject(arg0).cache = __wbindgen_enum_RequestCache[arg1];
        },
        __wbg_set_credentials_fa9c491a27c4bdf0: function(arg0, arg1) {
            getObject(arg0).credentials = __wbindgen_enum_RequestCredentials[arg1];
        },
        __wbg_set_dca99999bba88a9a: function(arg0, arg1, arg2) {
            getObject(arg0)[arg1 >>> 0] = takeObject(arg2);
        },
        __wbg_set_headers_7c1e39ece7826bec: function(arg0, arg1) {
            getObject(arg0).headers = getObject(arg1);
        },
        __wbg_set_integrity_30981c8c202903f4: function(arg0, arg1, arg2) {
            getObject(arg0).integrity = getStringFromWasm0(arg1, arg2);
        },
        __wbg_set_method_7a6811dec7a4feff: function(arg0, arg1, arg2) {
            getObject(arg0).method = getStringFromWasm0(arg1, arg2);
        },
        __wbg_set_mode_c90e3667002857d4: function(arg0, arg1) {
            getObject(arg0).mode = __wbindgen_enum_RequestMode[arg1];
        },
        __wbg_set_onmessage_ad00166b07fad0be: function(arg0, arg1) {
            getObject(arg0).onmessage = getObject(arg1);
        },
        __wbg_set_redirect_55da4f54e5bdbb0c: function(arg0, arg1) {
            getObject(arg0).redirect = __wbindgen_enum_RequestRedirect[arg1];
        },
        __wbg_set_referrer_d2c1743cb1404fd3: function(arg0, arg1, arg2) {
            getObject(arg0).referrer = getStringFromWasm0(arg1, arg2);
        },
        __wbg_set_referrer_policy_0ad167d0b0f0713e: function(arg0, arg1) {
            getObject(arg0).referrerPolicy = __wbindgen_enum_ReferrerPolicy[arg1];
        },
        __wbg_set_signal_d9da62b3f215c821: function(arg0, arg1) {
            getObject(arg0).signal = getObject(arg1);
        },
        __wbg_signal_e03304a84df9ed09: function(arg0) {
            const ret = getObject(arg0).signal;
            return addHeapObject(ret);
        },
        __wbg_stack_3b0d974bbf31e44f: function(arg0, arg1) {
            const ret = getObject(arg1).stack;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_startWorkers_8b582d57e92bd2d4: function(arg0, arg1, arg2) {
            const ret = startWorkers(takeObject(arg0), takeObject(arg1), wbg_rayon_PoolBuilder.__wrap(arg2));
            return addHeapObject(ret);
        },
        __wbg_static_accessor_GLOBAL_9d53f2689e622ca1: function() {
            const ret = typeof global === 'undefined' ? null : global;
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        },
        __wbg_static_accessor_GLOBAL_THIS_a1a35cec07001a8a: function() {
            const ret = typeof globalThis === 'undefined' ? null : globalThis;
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        },
        __wbg_static_accessor_SELF_4c59f6c7ea29a144: function() {
            const ret = typeof self === 'undefined' ? null : self;
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        },
        __wbg_static_accessor_WINDOW_e70ae9f2eb052253: function() {
            const ret = typeof window === 'undefined' ? null : window;
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        },
        __wbg_status_00549d55b78d949e: function(arg0) {
            const ret = getObject(arg0).status;
            return ret;
        },
        __wbg_subarray_4aa221f6a4f5ab22: function(arg0, arg1, arg2) {
            const ret = getObject(arg0).subarray(arg1 >>> 0, arg2 >>> 0);
            return addHeapObject(ret);
        },
        __wbg_then_18f476d590e58992: function(arg0, arg1, arg2) {
            const ret = getObject(arg0).then(getObject(arg1), getObject(arg2));
            return addHeapObject(ret);
        },
        __wbg_then_47213a40b6aeb86c: function(arg0, arg1) {
            const ret = getObject(arg0).then(getObject(arg1));
            return addHeapObject(ret);
        },
        __wbg_then_ac7b025999b52837: function(arg0, arg1) {
            const ret = getObject(arg0).then(getObject(arg1));
            return addHeapObject(ret);
        },
        __wbg_toString_1fcc5569307bd3f3: function(arg0) {
            const ret = getObject(arg0).toString();
            return addHeapObject(ret);
        },
        __wbg_value_2b11d753e2be3e57: function(arg0) {
            const ret = getObject(arg0).value;
            return addHeapObject(ret);
        },
        __wbg_value_f3625092ee4b37f4: function(arg0) {
            const ret = getObject(arg0).value;
            return addHeapObject(ret);
        },
        __wbg_versions_215a3ab1c9d5745a: function(arg0) {
            const ret = getObject(arg0).versions;
            return addHeapObject(ret);
        },
        __wbg_view_d523e3b92648b62c: function(arg0) {
            const ret = getObject(arg0).view;
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        },
        __wbg_waitAsync_bfb213899274180a: function(arg0, arg1, arg2) {
            const ret = Atomics.waitAsync(getObject(arg0), arg1 >>> 0, arg2);
            return addHeapObject(ret);
        },
        __wbg_waitAsync_fbc667ccb52b6fbf: function() {
            const ret = Atomics.waitAsync;
            return addHeapObject(ret);
        },
        __wbg_walletsummary_new: function(arg0) {
            const ret = WalletSummary.__wrap(arg0);
            return addHeapObject(ret);
        },
        __wbg_warn_410c3261e3c6d686: function(arg0) {
            console.warn(getObject(arg0));
        },
        __wbg_warn_c49a7a9581bf8bea: function(arg0, arg1, arg2, arg3) {
            console.warn(getObject(arg0), getObject(arg1), getObject(arg2), getObject(arg3));
        },
        __wbindgen_cast_0000000000000001: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [Externref], shim_idx: 1607, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, __wasm_bindgen_func_elem_4325);
            return addHeapObject(ret);
        },
        __wbindgen_cast_0000000000000002: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [Externref], shim_idx: 3201, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, __wasm_bindgen_func_elem_8954);
            return addHeapObject(ret);
        },
        __wbindgen_cast_0000000000000003: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [Externref], shim_idx: 3203, ret: Result(Unit), inner_ret: Some(Result(Unit)) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, __wasm_bindgen_func_elem_8958);
            return addHeapObject(ret);
        },
        __wbindgen_cast_0000000000000004: function(arg0, arg1) {
            // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [], shim_idx: 1578, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
            const ret = makeMutClosure(arg0, arg1, __wasm_bindgen_func_elem_4260);
            return addHeapObject(ret);
        },
        __wbindgen_cast_0000000000000005: function(arg0) {
            // Cast intrinsic for `F64 -> Externref`.
            const ret = arg0;
            return addHeapObject(ret);
        },
        __wbindgen_cast_0000000000000006: function(arg0, arg1) {
            // Cast intrinsic for `Ref(Slice(U8)) -> NamedExternref("Uint8Array")`.
            const ret = getArrayU8FromWasm0(arg0, arg1);
            return addHeapObject(ret);
        },
        __wbindgen_cast_0000000000000007: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return addHeapObject(ret);
        },
        __wbindgen_cast_0000000000000008: function(arg0) {
            // Cast intrinsic for `U64 -> Externref`.
            const ret = BigInt.asUintN(64, arg0);
            return addHeapObject(ret);
        },
        __wbindgen_cast_0000000000000009: function(arg0, arg1) {
            var v0 = getArrayU8FromWasm0(arg0, arg1).slice();
            wasm.__wbindgen_export4(arg0, arg1 * 1, 1);
            // Cast intrinsic for `Vector(U8) -> Externref`.
            const ret = v0;
            return addHeapObject(ret);
        },
        __wbindgen_link_e2b5a1199ad11c6b: function(arg0) {
            const val = `onmessage = function (ev) {
                let [ia, index, value] = ev.data;
                ia = new Int32Array(ia.buffer);
                let result = Atomics.wait(ia, index, value);
                postMessage(result);
            };
            `;
            const ret = typeof URL.createObjectURL === 'undefined' ? "data:application/javascript," + encodeURIComponent(val) : URL.createObjectURL(new Blob([val], { type: "text/javascript" }));
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbindgen_object_clone_ref: function(arg0) {
            const ret = getObject(arg0);
            return addHeapObject(ret);
        },
        __wbindgen_object_drop_ref: function(arg0) {
            takeObject(arg0);
        },
        memory: memory || new WebAssembly.Memory({initial:834,maximum:65536,shared:true}),
    };
    return {
        __proto__: null,
        "./zcash_wallet_wasm_bg.js": import0,
    };
}

function __wasm_bindgen_func_elem_4260(arg0, arg1) {
    wasm.__wasm_bindgen_func_elem_4260(arg0, arg1);
}

function __wasm_bindgen_func_elem_4325(arg0, arg1, arg2) {
    wasm.__wasm_bindgen_func_elem_4325(arg0, arg1, addHeapObject(arg2));
}

function __wasm_bindgen_func_elem_8954(arg0, arg1, arg2) {
    wasm.__wasm_bindgen_func_elem_8954(arg0, arg1, addHeapObject(arg2));
}

function __wasm_bindgen_func_elem_8958(arg0, arg1, arg2) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.__wasm_bindgen_func_elem_8958(retptr, arg0, arg1, addHeapObject(arg2));
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        if (r1) {
            throw takeObject(r0);
        }
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

function __wasm_bindgen_func_elem_8986(arg0, arg1, arg2, arg3) {
    wasm.__wasm_bindgen_func_elem_8986(arg0, arg1, addHeapObject(arg2), addHeapObject(arg3));
}


const __wbindgen_enum_ReadableStreamType = ["bytes"];


const __wbindgen_enum_ReferrerPolicy = ["", "no-referrer", "no-referrer-when-downgrade", "origin", "origin-when-cross-origin", "unsafe-url", "same-origin", "strict-origin", "strict-origin-when-cross-origin"];


const __wbindgen_enum_RequestCache = ["default", "no-store", "reload", "no-cache", "force-cache", "only-if-cached"];


const __wbindgen_enum_RequestCredentials = ["omit", "same-origin", "include"];


const __wbindgen_enum_RequestMode = ["same-origin", "no-cors", "cors", "navigate"];


const __wbindgen_enum_RequestRedirect = ["follow", "error", "manual"];
const BlockRangeFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_blockrange_free(ptr, 1));
const IntoUnderlyingByteSourceFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_intounderlyingbytesource_free(ptr, 1));
const IntoUnderlyingSinkFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_intounderlyingsink_free(ptr, 1));
const IntoUnderlyingSourceFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_intounderlyingsource_free(ptr, 1));
const PaymentRequestFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_paymentrequest_free(ptr, 1));
const PcztFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_pczt_free(ptr, 1));
const ProofGenerationKeyFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_proofgenerationkey_free(ptr, 1));
const ProposalFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_proposal_free(ptr, 1));
const SeedFingerprintFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_seedfingerprint_free(ptr, 1));
const TransactionRequestFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_transactionrequest_free(ptr, 1));
const UnifiedFullViewingKeyFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_unifiedfullviewingkey_free(ptr, 1));
const UnifiedSpendingKeyFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_unifiedspendingkey_free(ptr, 1));
const WalletSummaryFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_walletsummary_free(ptr, 1));
const WebWalletFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_webwallet_free(ptr, 1));
const wbg_rayon_PoolBuilderFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wbg_rayon_poolbuilder_free(ptr, 1));

function addHeapObject(obj) {
    if (heap_next === heap.length) heap.push(heap.length + 1);
    const idx = heap_next;
    heap_next = heap[idx];

    heap[idx] = obj;
    return idx;
}

function _assertClass(instance, klass) {
    if (!(instance instanceof klass)) {
        throw new Error(`expected instance of ${klass.name}`);
    }
}

const CLOSURE_DTORS = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(state => wasm.__wbindgen_export5(state.a, state.b));

function debugString(val) {
    // primitive types
    const type = typeof val;
    if (type == 'number' || type == 'boolean' || val == null) {
        return  `${val}`;
    }
    if (type == 'string') {
        return `"${val}"`;
    }
    if (type == 'symbol') {
        const description = val.description;
        if (description == null) {
            return 'Symbol';
        } else {
            return `Symbol(${description})`;
        }
    }
    if (type == 'function') {
        const name = val.name;
        if (typeof name == 'string' && name.length > 0) {
            return `Function(${name})`;
        } else {
            return 'Function';
        }
    }
    // objects
    if (Array.isArray(val)) {
        const length = val.length;
        let debug = '[';
        if (length > 0) {
            debug += debugString(val[0]);
        }
        for(let i = 1; i < length; i++) {
            debug += ', ' + debugString(val[i]);
        }
        debug += ']';
        return debug;
    }
    // Test for built-in
    const builtInMatches = /\[object ([^\]]+)\]/.exec(toString.call(val));
    let className;
    if (builtInMatches && builtInMatches.length > 1) {
        className = builtInMatches[1];
    } else {
        // Failed to match the standard '[object ClassName]'
        return toString.call(val);
    }
    if (className == 'Object') {
        // we're a user defined class or Object
        // JSON.stringify avoids problems with cycles, and is generally much
        // easier than looping through ownProperties of `val`.
        try {
            return 'Object(' + JSON.stringify(val) + ')';
        } catch (_) {
            return 'Object';
        }
    }
    // errors
    if (val instanceof Error) {
        return `${val.name}: ${val.message}\n${val.stack}`;
    }
    // TODO we could test for more things here, like `Set`s and `Map`s.
    return className;
}

function dropObject(idx) {
    if (idx < 1028) return;
    heap[idx] = heap_next;
    heap_next = idx;
}

function getArrayJsValueFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    const mem = getDataViewMemory0();
    const result = [];
    for (let i = ptr; i < ptr + 4 * len; i += 4) {
        result.push(takeObject(mem.getUint32(i, true)));
    }
    return result;
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer !== wasm.memory.buffer) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.buffer !== wasm.memory.buffer) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function getObject(idx) { return heap[idx]; }

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        wasm.__wbindgen_export3(addHeapObject(e));
    }
}

let heap = new Array(1024).fill(undefined);
heap.push(undefined, null, true, false);

let heap_next = heap.length;

function isLikeNone(x) {
    return x === undefined || x === null;
}

function makeMutClosure(arg0, arg1, f) {
    const state = { a: arg0, b: arg1, cnt: 1 };
    const real = (...args) => {

        // First up with a closure we increment the internal reference
        // count. This ensures that the Rust closure environment won't
        // be deallocated while we're invoking it.
        state.cnt++;
        const a = state.a;
        state.a = 0;
        try {
            return f(a, state.b, ...args);
        } finally {
            state.a = a;
            real._wbg_cb_unref();
        }
    };
    real._wbg_cb_unref = () => {
        if (--state.cnt === 0) {
            wasm.__wbindgen_export5(state.a, state.b);
            state.a = 0;
            CLOSURE_DTORS.unregister(state);
        }
    };
    CLOSURE_DTORS.register(real, state, state);
    return real;
}

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passArrayJsValueToWasm0(array, malloc) {
    const ptr = malloc(array.length * 4, 4) >>> 0;
    const mem = getDataViewMemory0();
    for (let i = 0; i < array.length; i++) {
        mem.setUint32(ptr + 4 * i, addHeapObject(array[i]), true);
    }
    WASM_VECTOR_LEN = array.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeObject(idx) {
    const ret = getObject(idx);
    dropObject(idx);
    return ret;
}

let cachedTextDecoder = (typeof TextDecoder !== 'undefined' ? new TextDecoder('utf-8', { ignoreBOM: true, fatal: true }) : undefined);
if (cachedTextDecoder) cachedTextDecoder.decode();

const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().slice(ptr, ptr + len));
}

const cachedTextEncoder = (typeof TextEncoder !== 'undefined' ? new TextEncoder() : undefined);

if (cachedTextEncoder) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module, thread_stack_size) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    if (typeof thread_stack_size !== 'undefined' && (typeof thread_stack_size !== 'number' || thread_stack_size === 0 || thread_stack_size % 65536 !== 0)) {
        throw new Error('invalid stack size');
    }

    wasm.__wbindgen_start(thread_stack_size);
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module, memory) {
    if (wasm !== undefined) return wasm;

    let thread_stack_size
    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module, memory, thread_stack_size} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports(memory);
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module, thread_stack_size);
}

async function __wbg_init(module_or_path, memory) {
    if (wasm !== undefined) return wasm;

    let thread_stack_size
    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path, memory, thread_stack_size} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('zcash_wallet_wasm_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports(memory);

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module, thread_stack_size);
}

export { initSync, __wbg_init as default };
