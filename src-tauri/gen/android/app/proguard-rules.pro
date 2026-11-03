# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile
# Ядро sing-box (libbox): Go вызывает методы наших классов (PlatformInterface и др.)
# через JNI по именам. Классы самого libbox сохраняет proguard.txt внутри .aar.
-keep class * implements io.nekohasekai.libbox.** { *; }

# Сканер QR-кодов (ML Kit): ищет свои компоненты по классам, а R8 сливает и переименовывает
# их - getClient падает с NullPointerException в релизной сборке. Не трогаем ML Kit целиком.
-keep class com.google.mlkit.** { *; }
-keep class com.google.android.gms.internal.mlkit_** { *; }
-keep class com.google.firebase.components.** { *; }
