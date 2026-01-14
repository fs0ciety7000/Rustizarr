mod plex;
mod tmdb;
mod image_ops;

use axum::{
    routing::{get, post},
    Json, Router, Extension,
    extract::{Path as AxumPath, Multipart},
    body::Body, // Pour renvoyer l'image brute
    response::IntoResponse,
    http::{HeaderMap, header, StatusCode},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::path::Path;
use std::time::{Duration, Instant};
use std::io::Cursor;
use dotenv::dotenv;
use std::env;
use tower_http::cors::CorsLayer;
use plex::{PlexClient, PlexMovie}; 
use tmdb::TmdbClient;
use image_ops::ImageProcessor;

#[derive(Clone, Serialize, Deserialize)]
struct AppConfig {
    plex_url: String,
    plex_token: String,
    tmdb_key: String,
    library_id: String, 
}

struct AppState {
    config: Mutex<AppConfig>,
    library_cache: Mutex<LibraryCache>,
}


struct LibraryCache {
    movies: Vec<PlexMovie>,
    last_update: Option<Instant>,
    cache_duration: Duration,
}

impl LibraryCache {
    fn new() -> Self {
        Self {
            movies: Vec::new(),
            last_update: None,
            cache_duration: Duration::from_secs(300), // 5 minutes
        }
    }

    fn is_valid(&self) -> bool {
        if let Some(last) = self.last_update {
            last.elapsed() < self.cache_duration
        } else {
            false
        }
    }

    fn update(&mut self, movies: Vec<PlexMovie>) {
        self.movies = movies;
        self.last_update = Some(Instant::now());
    }

    fn get(&self) -> Option<Vec<PlexMovie>> {
        if self.is_valid() {
            Some(self.movies.clone())
        } else {
            None
        }
    }

    fn invalidate(&mut self) {
        self.last_update = None;
    }
}


// --- STRUCTURES POUR LE WEBHOOK PLEX ---
#[derive(Deserialize, Debug)]
struct PlexWebhookPayload {
    event: String,
    #[serde(rename = "Metadata")]
    metadata: Option<WebhookMetadata>,
}

#[derive(Deserialize, Debug)]
struct WebhookMetadata {
    #[serde(rename = "ratingKey")]
    rating_key: String,
    #[serde(rename = "type")]
    media_type: String,
}
// ---------------------------------------

#[tokio::main]
async fn main() {
    dotenv().ok();

    let config = AppConfig {
        plex_url: env::var("PLEX_URL").expect("❌ PLEX_URL manquant dans .env"),
        plex_token: env::var("PLEX_TOKEN").expect("❌ PLEX_TOKEN manquant dans .env"),
        tmdb_key: env::var("TMDB_KEY").expect("❌ TMDB_KEY manquant dans .env"),
        library_id: env::var("LIBRARY_ID").unwrap_or("1".to_string()),
    };

    let app_state = Arc::new(AppState {
        config: Mutex::new(config),
        library_cache: Mutex::new(LibraryCache::new()),
    });

    let app = Router::new()
        .route("/", get(|| async { "RustOverlay Backend Running 🚀" }))
        .route("/scan", get(run_full_library_scan))      // Scan manuel complet
        .route("/webhook", post(handle_plex_webhook))    // Automatisation
        .route("/api/library", get(get_library_json))
        .route("/api/library/refresh", post(refresh_library_cache)) 
        .route("/api/image/:id", get(get_plex_image))
        .layer(CorsLayer::permissive())
        .layer(Extension(app_state));

    println!("🚀 Serveur lancé sur http://0.0.0.0:3000");
    println!("👉 Endpoint Webhook : http://TON_IP_LOCALE:3000/webhook");
    if let Ok(cwd) = std::env::current_dir() { println!("📂 Dossier d'exécution (CWD) : {:?}", cwd); }

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// ==================================================================================
// 1. GESTION DU WEBHOOK (AUTOMATISATION)
// ==================================================================================

async fn handle_plex_webhook(
    Extension(state): Extension<Arc<AppState>>,
    mut multipart: Multipart,
) {
    while let Ok(Some(field)) = multipart.next_field().await {
        // Correction ici : on gère le cas où name() renvoie None
        if field.name().unwrap_or("") == "payload" {
            if let Ok(text) = field.text().await {
                if let Ok(payload) = serde_json::from_str::<PlexWebhookPayload>(&text) {
                    
                    // On ne s'intéresse qu'aux nouveaux ajouts ("library.new") de type film
                    if payload.event == "library.new" {
                        if let Some(meta) = payload.metadata {
                            if meta.media_type == "movie" {
                                println!("🔔 Webhook : Nouveau film détecté (ID: {})", meta.rating_key);
                                
                                let state_clone = state.clone();
                                tokio::spawn(async move {
                                    process_single_movie_by_id(state_clone, meta.rating_key).await;
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn process_single_movie_by_id(state: Arc<AppState>, rating_key: String) {
    let config = state.config.lock().await;
    let plex = PlexClient::new(config.plex_url.clone(), config.plex_token.clone());
    let tmdb = TmdbClient::new(config.tmdb_key.clone());
    drop(config);

    println!("⏳ Attente de 10s pour l'analyse Plex...");
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;

    match plex.get_item_details(&rating_key).await {
        Ok(movie) => {
            if let Ok(_) = process_movie_logic(&plex, &tmdb, movie).await {
                // Invalide le cache après traitement réussi
                println!("🔄 Invalidation du cache suite au traitement...");
                let mut cache = state.library_cache.lock().await;
                cache.invalidate();
            }
        },
        Err(e) => println!("❌ Erreur Webhook (Détails film) : {:?}", e),
    }
}

// ==================================================================================
// 2. SCAN MANUEL COMPLET (/scan)
// ==================================================================================

async fn run_full_library_scan(Extension(state): Extension<Arc<AppState>>) -> Json<String> {
    let config = state.config.lock().await;
    let plex = PlexClient::new(config.plex_url.clone(), config.plex_token.clone());
    let tmdb = TmdbClient::new(config.tmdb_key.clone());
    let library_id = config.library_id.clone();
    drop(config);

    println!("Connexion Plex...");

    match plex.get_library_items(&library_id).await {
        Ok(movies) => {
            let total = movies.len();
            println!("🔍 Analyse de la bibliothèque : {} films trouvés.", total);
            let mut report = String::new();

            for (index, summary_movie) in movies.iter().enumerate() {
                println!("---------------------------------------------------");
                println!("🔎 Analyse ({}/{}) : {}", index + 1, total, summary_movie.title);

                match plex.get_item_details(&summary_movie.rating_key).await {
                    Ok(movie) => {
                        let already_processed = movie.has_label("Rustizarr");

                        if already_processed {
                            println!("   ⏭️  SKIP : Film déjà traité (Label 'Rustizarr' trouvé).");
                            continue;
                        }

                        println!("   ✨ Nouveau film détecté, lancement du traitement...");
                        if let Ok(msg) = process_movie_logic(&plex, &tmdb, movie).await {
                            report.push_str(&msg);
                        }
                    },
                    Err(e) => println!("   ⚠️ Erreur récupération détails: {:?}, passage au suivant.", e),
                };
            }
            
            // Invalide le cache à la fin du scan
            println!("🔄 Scan terminé, invalidation du cache...");
            let mut cache = state.library_cache.lock().await;
            cache.invalidate();
            
            Json(report)
        }
        Err(e) => Json(format!("Erreur Plex: {:?}", e))
    }
}

// ==================================================================================
// 3. CŒUR DU SYSTÈME (LOGIQUE DE TRAITEMENT)
// ==================================================================================

async fn process_movie_logic(plex: &PlexClient, tmdb: &TmdbClient, movie: PlexMovie) -> anyhow::Result<String> {
    
    let tmdb_id_opt = if let Some(forced_id) = get_forced_tmdb_id(&movie.title) {
        println!("   🔧 OVERRIDE MANUEL ACTIVÉ : Utilisation de l'ID {}", forced_id);
        Some(forced_id)
    } else {
        PlexClient::extract_tmdb_id(&movie)
    };

    if let Some(tmdb_id) = tmdb_id_opt {

        let mut final_url = None;
        match tmdb.get_textless_poster(&tmdb_id).await {
            Ok(Some(url)) => final_url = Some(url),
            Ok(None) => {
                println!("   ⚠️ Pas de poster textless. Tentative poster standard...");
                if let Ok(Some(std_url)) = tmdb.get_standard_poster(&tmdb_id).await {
                    final_url = Some(std_url);
                }
            }
            Err(e) => println!("   ❌ Erreur API TMDB : {:?}", e),
        }

        if let Some(url) = final_url {
            println!("   📸 Poster trouvé, téléchargement...");
                
            if let Ok(mut poster) = ImageProcessor::download_image(&url).await {
                
                let _ = ImageProcessor::add_gradient_masks(poster.clone()).map(|img| poster = img);
                let _ = ImageProcessor::add_inner_glow_border(poster.clone()).map(|img| poster = img);
                let _ = ImageProcessor::add_movie_title(poster.clone(), &movie.title).map(|img| poster = img);

                let base_path = Path::new("../overlays/media_info");
                let mut top_left_index = 0;

                if let Some(media_list) = &movie.media {
                    if let Some(media) = media_list.first() {
                        if let Some(res_file) = get_resolution_filename(media) {
                            let path = base_path.join("resolution").join(res_file);
                            if let Ok(img) = ImageProcessor::add_overlay(poster.clone(), &path, top_left_index, false, 0.065) {
                                poster = img;
                                top_left_index += 1;
                            }
                        }
                    }
                }

                if let Some(edition_file) = get_edition_filename(&movie) {
                    let path = base_path.join("edition").join(edition_file);
                    if let Ok(img) = ImageProcessor::add_overlay(poster.clone(), &path, top_left_index, false, 0.065) {
                        poster = img;
                    }
                }

                if let Some(media_list) = &movie.media {
                    if let Some(media) = media_list.first() {
                        if let Some(audio_file) = get_codec_combo_filename(media) {
                            let path = base_path.join("codec").join(audio_file);
                            if let Ok(img) = ImageProcessor::add_overlay(poster.clone(), &path, 0, true, 0.050) {
                                poster = img;
                            }
                        }
                    }
                }

                if let Some(rating) = movie.audience_rating {
                    let audience_path = Path::new("../overlays/audience_score");
                    let badge_file = get_audience_badge_filename(rating);
                    let full_path = audience_path.join(badge_file);
                    let _ = ImageProcessor::add_overlay_bottom_right(poster.clone(), &full_path, 0.065, Some(rating))
                        .map(|img| poster = img);
                }

                let rgb_poster = poster.to_rgb8(); 
                let mut bytes: Vec<u8> = Vec::new();
                rgb_poster.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Jpeg).unwrap();

                match plex.upload_poster(&movie.rating_key, bytes).await {
                    Ok(_) => {
                        let msg = format!("✅ SUCCÈS : '{}'\n", movie.title);
                        println!("{}", msg);
                        println!("   🏷️ Ajout du label 'Rustizarr'...");
                        if let Err(e) = plex.add_label(&movie.rating_key, "Rustizarr").await {
                            println!("      ⚠️ Echec ajout label : {:?}", e);
                        }
                        return Ok(msg);
                    },
                    Err(e) => {
                        println!("❌ Erreur upload Plex : {:?}", e);
                        return Err(anyhow::anyhow!("Erreur upload"));
                    },
                }
            }
        } else {
            println!("   ❌ ABANDON : Aucune image trouvée sur TMDB.");
        }
    } else {
        println!("   ⚠️ Pas d'ID TMDB trouvé.");
    }
    
    Ok("Film ignoré ou échec partiel".to_string())
}

// ==================================================================================
// 4. FONCTIONS UTILITAIRES (HELPERS)
// ==================================================================================

fn get_forced_tmdb_id(title: &str) -> Option<String> {
    match title.to_lowercase().as_str() {
        "abyss" => Some("1025527".to_string()), 
        "kingsman : le cercle d'or" | "kingsman the golden circle" => Some("343668".to_string()),
        _ => None
    }
}

fn get_edition_filename(movie: &plex::PlexMovie) -> Option<&str> {
    let t = movie.title.to_lowercase();
    if t.contains("director's cut") || t.contains("director cut") { Some("Directors-Cut.png") }
    else if t.contains("extended") { Some("Extended-Edition.png") }
    else if t.contains("remastered") { Some("Remastered.png") }
    else if t.contains("uncut") { Some("Uncut.png") }
    else if t.contains("imax") { Some("IMAX.png") }
    else { None }
}

fn get_resolution_filename(media: &plex::PlexMedia) -> Option<String> {
    let raw_res = media.video_resolution.as_deref().unwrap_or("").to_lowercase();
    match raw_res.as_str() {
        "4k" | "ultra hd" => Some("Ultra-HD.png".to_string()),
        "1080" | "1080p" | "fhd" => Some("1080P.png".to_string()), 
        _ => None, 
    }
}

fn get_audience_badge_filename(rating: f64) -> &'static str {
    if rating >= 8.0 { "audience_score_high.png" } 
    else if rating >= 6.0 { "audience_score_mid.png" } 
    else { "audience_score_low.png" }
}

fn get_codec_combo_filename(media: &plex::PlexMedia) -> Option<String> {
    let fallback_audio = media.audio_codec.as_deref().unwrap_or("").to_lowercase();
    
    let mut has_streams_access = false;
    let mut is_dv = false;
    let mut is_hdr = false;
    let mut is_plus = false;
    let mut has_atmos = false;
    let mut has_truehd = false;
    let mut has_dts_hd = false;
    let mut has_dts_x = false;
    let mut has_dd_plus = false; 
    let mut found_audio_codec = String::new();

    if let Some(parts_value) = &media.parts {
        let parts_slice = if let Some(arr) = parts_value.as_array() { arr.as_slice() } else { std::slice::from_ref(parts_value) };

        for part in parts_slice {
            let maybe_streams = part.get("Stream").or_else(|| part.get("stream"));
            if let Some(streams_value) = maybe_streams {
                has_streams_access = true;
                let streams_slice = if let Some(arr) = streams_value.as_array() { arr.as_slice() } else { std::slice::from_ref(streams_value) };

                for stream in streams_slice {
                    let stream_type = stream.get("streamType").and_then(|v| v.as_u64()).unwrap_or(0);
                    
                    if stream_type == 1 { // VIDEO
                        let display = stream.get("displayTitle").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                        let title = stream.get("title").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                        if stream.get("doviprofile").is_some() || stream.get("DOVIProfile").is_some() || stream.get("DOVIPresent").is_some() { is_dv = true; }
                        if display.contains("dolby vision") || title.contains("dolby vision") || display.contains("dovi") || title.contains("dovi") { is_dv = true; }
                        if display.contains("hdr10+") || title.contains("hdr10+") { is_plus = true; }
                        else if display.contains("hdr") || title.contains("hdr") { is_hdr = true; }
                    }

                    if stream_type == 2 { // AUDIO
                        let display = stream.get("displayTitle").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                        let title = stream.get("title").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                        let codec = stream.get("codec").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                        let profile = stream.get("audioProfile").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                        found_audio_codec = codec.clone();

                        if title.contains("atmos") || display.contains("atmos") { has_atmos = true; }
                        match codec.as_str() {
                            "truehd" => has_truehd = true,
                            "dca" | "dts" => {
                                if profile == "dts:x" { has_dts_x = true; }
                                // Fallback : On marque tout DTS comme HD car pas de badge simple
                                has_dts_hd = true; 
                            },
                            "eac3" | "ac3" => has_dd_plus = true,
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    let video_part = if has_streams_access {
        if is_dv && is_hdr { Some("DV-HDR") }
        else if is_dv && is_plus { Some("DV-Plus") }
        else if is_dv { Some("DV") }
        else if is_plus { Some("Plus") }
        else if is_hdr { Some("HDR") }
        else { None }
    } else { None };

    let audio_part = if has_streams_access {
        if has_truehd && has_atmos { Some("TrueHD-Atmos") }
        else if has_truehd { Some("TrueHD") }
        else if has_dts_x { Some("DTS-X") }
        else if has_dts_hd { Some("DTS-HD") }
        else if has_atmos { Some("Atmos") }
        else if has_dd_plus { Some("DigitalPlus") }
        else { None }
    } else {
        match fallback_audio.as_str() {
            "truehd" => Some("TrueHD"),
            "dca" | "dts" => Some("DTS-HD"),
            "eac3" | "ac3" => Some("DigitalPlus"),
            _ => None
        }
    };

    let result = match (video_part, audio_part) {
        (Some(v), Some(a)) => Some(format!("{}-{}.png", v, a)),
        (Some(v), None) => Some(format!("{}.png", v)),
        (None, Some(a)) => Some(format!("{}.png", a)),
        (None, None) => None,
    };

    if result.is_none() && has_streams_access {
        if !found_audio_codec.contains("aac") && !found_audio_codec.contains("mp3") {
             println!("      ℹ️ Info: Codec audio '{}' détecté, mais aucun badge combiné généré.", found_audio_codec);
        }
    }

    result
}

// ==================================================================================
// 5. API POUR LE FRONTEND
// ==================================================================================

async fn get_library_json(Extension(state): Extension<Arc<AppState>>) -> Json<Vec<PlexMovie>> {
       // Vérifie le cache d'abord
    {
        let cache = state.library_cache.lock().await;
        if let Some(cached_movies) = cache.get() {
            let count_processed = cached_movies.iter()
                .filter(|m| m.has_label("Rustizarr"))
                .count();
            
            println!("💾 Cache HIT : {} films (dont {} traités)", 
                cached_movies.len(), count_processed);
            
            return Json(cached_movies);
        }
    }

    // Cache invalide, recharge les données
    println!("🔄 Cache MISS : Rechargement des données...");

    let config = state.config.lock().await;
    let plex = PlexClient::new(config.plex_url.clone(), config.plex_token.clone());
let library_id = config.library_id.clone();
    drop(config); // Libère le lock


  match plex.get_library_items_with_labels(&library_id).await {
        Ok(movies) => {
            let count_processed = movies.iter()
                .filter(|m| m.has_label("Rustizarr"))
                .count();
            
            println!("✅ Données chargées : {} films (dont {} traités)", 
                movies.len(), count_processed);
            
            // Met à jour le cache
            let mut cache = state.library_cache.lock().await;
            cache.update(movies.clone());
            
            Json(movies)
        },
        Err(e) => {
            println!("❌ Erreur récupération librairie : {:?}", e);
            Json(vec![])
        }
    }
}


// 5. NOUVEAU endpoint pour forcer le rafraîchissement
async fn refresh_library_cache(Extension(state): Extension<Arc<AppState>>) -> Json<serde_json::Value> {
    println!("🔄 Rafraîchissement manuel du cache demandé...");
    
    // Invalide le cache
    {
        let mut cache = state.library_cache.lock().await;
        cache.invalidate();
    }
    
    // Recharge les données (appellera get_library_json en interne)
    let config = state.config.lock().await;
    let plex = PlexClient::new(config.plex_url.clone(), config.plex_token.clone());
    let library_id = config.library_id.clone();
    drop(config);

    match plex.get_library_items_with_labels(&library_id).await {
        Ok(movies) => {
            let count_processed = movies.iter()
                .filter(|m| m.has_label("Rustizarr"))
                .count();
            
            let mut cache = state.library_cache.lock().await;
            cache.update(movies.clone());
            
            Json(serde_json::json!({
                "success": true,
                "total": movies.len(),
                "processed": count_processed,
                "message": "Cache rafraîchi avec succès"
            }))
        },
        Err(e) => {
            Json(serde_json::json!({
                "success": false,
                "error": format!("{:?}", e)
            }))
        }
    }
}


async fn get_plex_image(
    AxumPath(rating_key): AxumPath<String>,
    Extension(state): Extension<Arc<AppState>>,
) -> impl IntoResponse {
    let config = state.config.lock().await;
    
    let url = format!(
        "{}/library/metadata/{}/thumb?X-Plex-Token={}", 
        config.plex_url, 
        rating_key, 
        config.plex_token
    );
    
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();
    
    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(_) => return (StatusCode::NOT_FOUND, "Plex inaccessible").into_response(),
    };
    
    // Gestion redirection
    if resp.status().is_redirection() {
        if let Some(location) = resp.headers().get("location") {
            if let Ok(loc_str) = location.to_str() {
                let final_url = if loc_str.starts_with("http") {
                    loc_str.to_string()
                } else {
                    format!("{}{}", config.plex_url, loc_str)
                };
                
                if let Ok(final_resp) = client.get(&final_url).send().await {
                    return process_image_response(final_resp).await;
                }
            }
        }
        return (StatusCode::INTERNAL_SERVER_ERROR, "Erreur redirection").into_response();
    }
    
    // Succès direct
    if resp.status().is_success() {
        return process_image_response(resp).await;
    }
    
    (StatusCode::from_u16(resp.status().as_u16()).unwrap(), "Echec").into_response()
}

async fn process_image_response(resp: reqwest::Response) -> axum::response::Response {
    if !resp.status().is_success() {
        return (StatusCode::NOT_FOUND, "Image introuvable").into_response();
    }

    let content_type = resp.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/jpeg")
        .to_string();

    match resp.bytes().await {
        Ok(image_bytes) => {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, content_type.parse().unwrap());
            headers.insert(header::CACHE_CONTROL, "public, max-age=31536000".parse().unwrap());
            (StatusCode::OK, headers, Body::from(image_bytes)).into_response()
        },
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Erreur flux").into_response()
    }
}