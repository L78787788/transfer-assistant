package com.transassist.transfer_assistant

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

class IdentityKeyStore(private val context: Context) {
    fun wrappingKeyBase64(): String = Base64.encodeToString(loadOrCreateWrappingKey(), Base64.NO_WRAP)

    private fun loadOrCreateWrappingKey(): ByteArray {
        val preferences = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
        val encoded = preferences.getString(WRAPPED_KEY, null)
        if (encoded != null) {
            val protected = Base64.decode(encoded, Base64.NO_WRAP)
            require(protected.size > NONCE_BYTES) { "身份包装密钥已损坏" }
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(
                Cipher.DECRYPT_MODE,
                masterKey(),
                GCMParameterSpec(128, protected.copyOfRange(0, NONCE_BYTES)),
            )
            return cipher.doFinal(protected.copyOfRange(NONCE_BYTES, protected.size))
        }

        val wrappingKey = ByteArray(32).also(SecureRandom()::nextBytes)
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, masterKey())
        val protected = cipher.iv + cipher.doFinal(wrappingKey)
        preferences.edit()
            .putString(WRAPPED_KEY, Base64.encodeToString(protected, Base64.NO_WRAP))
            .apply()
        return wrappingKey
    }

    private fun masterKey(): SecretKey {
        val keyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (keyStore.getKey(KEY_ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        generator.init(
            KeyGenParameterSpec.Builder(
                KEY_ALIAS,
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
            )
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return generator.generateKey()
    }

    companion object {
        private const val PREFERENCES = "identity_security"
        private const val WRAPPED_KEY = "wrapped_key"
        private const val KEY_ALIAS = "transassist_identity_master_v1"
        private const val TRANSFORMATION = "AES/GCM/NoPadding"
        private const val NONCE_BYTES = 12
    }
}
