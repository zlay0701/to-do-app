package com.example.todo

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * 同步令牌的 Android 存储（SPEC §11）：密钥在 AndroidKeyStore 里，
 * 令牌经 AES-256-GCM 加密后才落盘到应用私有目录，明文永不写文件。
 * Rust 侧（src-tauri/src/android_secret.rs）经 JNI 调这三个静态方法。
 *
 * 注意：这是 gen/android 里手写的补充文件（init 生成的骨架不含它），
 * 若删除 gen/android 重新 init，需要从版本记录里恢复本文件。
 */
object TokenStore {
  private const val KEY_ALIAS = "todo-sync-token"
  private const val FILE_NAME = "sync_token.enc"
  private const val TRANSFORMATION = "AES/GCM/NoPadding"
  private const val GCM_TAG_BITS = 128
  private const val IV_BYTES = 12

  private fun keyStore(): KeyStore =
    KeyStore.getInstance("AndroidKeyStore").apply { load(null) }

  private fun existingKey(): SecretKey? =
    keyStore().getKey(KEY_ALIAS, null) as? SecretKey

  private fun getOrCreateKey(): SecretKey {
    existingKey()?.let { return it }
    val spec = KeyGenParameterSpec.Builder(
      KEY_ALIAS,
      KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
    )
      .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
      .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
      .setRandomizedEncryptionRequired(true)
      .build()
    return KeyGenerator
      .getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
      .apply { init(spec) }
      .generateKey()
  }

  private fun tokenFile(context: Context): File = File(context.filesDir, FILE_NAME)

  /** 无文件或密钥已丢时返回 null（调用方按"无令牌"处理）。 */
  @JvmStatic
  fun getToken(context: Context): String? {
    val file = tokenFile(context)
    if (!file.exists()) return null
    val blob = file.readBytes()
    if (blob.size <= IV_BYTES) return null
    val key = existingKey() ?: return null
    val cipher = Cipher.getInstance(TRANSFORMATION)
    cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(GCM_TAG_BITS, blob, 0, IV_BYTES))
    return String(cipher.doFinal(blob, IV_BYTES, blob.size - IV_BYTES), Charsets.UTF_8)
  }

  /** 加密后写盘：文件内容为 IV（12 字节）+ 密文。 */
  @JvmStatic
  fun setToken(context: Context, token: String) {
    val cipher = Cipher.getInstance(TRANSFORMATION)
    cipher.init(Cipher.ENCRYPT_MODE, getOrCreateKey())
    val encrypted = cipher.doFinal(token.toByteArray(Charsets.UTF_8))
    tokenFile(context).writeBytes(cipher.iv + encrypted)
  }

  @JvmStatic
  fun deleteToken(context: Context) {
    tokenFile(context).delete()
    runCatching { keyStore().deleteEntry(KEY_ALIAS) }
  }
}
