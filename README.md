# AetherMC — Modular High-Performance Minecraft Launcher

**Tauri v2 (Rust)** ve **React + TypeScript + Tailwind CSS** ile geliştirilmiş; boşta `<30MB` RAM hedefleyen, çevrimdışı (RFC 4122 v3 UUID) geliştirici kimlik doğrulaması sunan, CurseForge örneklerini gerçek zamanlı izleyip kopyalamadan çalıştıran, özel modpaketi oluşturucu ve bellek içi (`.jar` in-memory) meta veri okuyucu içeren, örnek bazlı RAM/JVM miras sistemi (Generational ZGC & Aikar G1GC), otomatik çökme analizi (Crash Analyzer + tek tıkla kurtarma) ve dinamik CSS Tema + JS Eklenti mimarisine sahip Minecraft başlatıcısı.

---

## Temel Mimari Özellikler

1. **Çevrimdışı Geliştirici Kimlik Doğrulama Motoru (`src-tauri/src/auth/`)**
   - Harici OAuth sunucularına ihtiyaç duymaz.
   - `MD5("OfflinePlayer:" + username)` algoritması ve RFC 4122 v3 bit maskelemesi ile deterministik UUID üretir.
   - Oyun başlatma argümanlarına `--accessToken 0` ve `--userType legacy` bayraklarını enjekte eder.
   - Çoklu yerel profil kaydı ve hızlı geçiş destekler.

2. **Manuel Sürüm İndirici, Asset Hattı & Özel Modpaketi Oluşturucu (`src-tauri/src/downloader/` & `src-tauri/src/instance/`)**
   - **Mojang Manifest v2:** `version_manifest_v2.json` üzerinden Release, Snapshot ve Historical (Beta/Alpha) sürümlerini filtreler ve indirir.
   - **Tam Asset & Natives Hattı:** `assetIndex` ve `assets/objects/` dosyalarını 32 paralel akış ve SHA-1 doğrulamasıyla indirir; 1.18.2 öncesi native `.dll`/`.so`/`.dylib` kütüphanelerini otomatik çıkarır ve Maven koordinat çakışmalarını (Modloader öncelikli) tekilleştirir.
   - **Modloader Entegrasyonu:** **Fabric** (`meta.fabricmc.net`), **Quilt** (`meta.quiltmc.org`), **NeoForge** (`maven.neoforged.net`) ve **Forge** meta API'lerini destekler.
   - **Bellek-İçi (In-Memory) JAR Okuyucu:** `zip` crate kullanarak diske hiçbir dosya çıkarmadan `.jar` ve `.jar.disabled` içindeki `fabric.mod.json`, `quilt.mod.json`, `META-INF/mods.toml` ve `META-INF/neoforge.mods.toml` dosyalarını RAM'de ayrıştırır.

3. **CurseForge Gerçek Zamanlı Dizin İzleyici & Oto-Adaptör (`src-tauri/src/instance/curseforge_watcher.rs`)**
   - `notify` crate ile CurseForge `Instances` klasörünü gerçek zamanlı izler.
   - `minecraftinstance.json` veya `manifest.json` tespit edildiğinde mod dosyalarını kopyalamadan `--gameDir "<cf_instance_path>"` ile yerinde başlatır.

4. **Örnek Bazlı RAM/JVM Miras Sistemi & Akıllı Java Seçici (`src-tauri/src/runner/`)**
   - `GlobalSettings` ve `InstanceConfig` arasında iki katmanlı miras (`override_memory`, `override_java`).
   - MC `<= 1.16.5` için **Java 8**, MC `1.17–1.20.4` için **Java 17**, MC `>= 1.20.5` için **Java 21** çalışma zamanını otomatik seçer; sistemde yoksa **Adoptium Temurin API** üzerinden indirir.
   - Java 21+ algılandığında **Generational ZGC** (`-XX:+UseZGC -XX:+ZGenerational`), Java 8/17 algılandığında **Aikar's G1GC** bayraklarını ve Log4Shell (`-Dlog4j2.formatMsgNoLookups=true`) korumasını otomatik ekler.

5. **Akıllı Çökme Analizcisi & Öz-İyileştirme (`src-tauri/src/crash/crash_analyzer.rs`)**
   - Ön-başlatma sırasında takılı `session.lock` dosyalarını ve asılı kalmış zombi `javaw.exe` süreçlerini temizler; bozuk veya 0 baytlık dosyaları `.part` + SHA-1 kontrolüyle onarır.
   - Oyun sıfır olmayan kodla kapandığında `latest.log` ve `crash-reports/` dosyalarını Regex ile tarayarak 5 ana kategoriyi yakalar ve tek tıkla kurtarma butonları sunar:
     - **Eksik Bağımlılık (`ModResolutionException`)**
     - **Mixin Çakışması (`MixinApplyError` -> Suçlu `.jar` tespiti ve devre dışı bırakıp yeniden başlatma)**
     - **Yetersiz Bellek (`OutOfMemoryError` -> Tek tıkla `+2048 MB` RAM artırma)**
     - **Java Sürüm Uyuşmazlığı (`UnsupportedClassVersionError` -> Tek tıkla Java runtime değiştirme)**
     - **Bozuk Config (`NightConfig` / `TomlParsingException` -> Tek tıkla `.bak` yedekleme ve sıfırlama)**

6. **Modlanabilir Tema (`themes/`) & Eklenti (`plugins/`) Mimarisi (`src-tauri/src/extensibility/`)**
   - Çalışma zamanında harici `.css` ve `theme.json` dosyalarını yükler.
   - Kum havuzlu (sandboxed) JS eklenti veri yolu: `onAppInit`, `onInstanceDetected`, `onPreLaunch`, `onGameLog`, `onGameCrash`.
   - Arayüzde anlık **Türkçe (TR) / İngilizce (EN)** dil geçişi bulunur.

---

## Geliştirme ve Çalıştırma Komutları

```bash
# Ön yüz bağımlılıklarını yükle
npm install

# Ön yüz üretim derlemesi
npm run build

# Rust arka uç birim testlerini çalıştır
cargo test --manifest-path src-tauri/Cargo.toml

# Tauri geliştirme modunda başlat
npm run tauri dev
```
