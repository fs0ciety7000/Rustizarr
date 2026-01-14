mod plex;
mod tmdb;
mod image_ops;
mod processor;

use axum::{
    routing::{get, post},
    Json, Router, Extension,
    extract::{Path as AxumPath, Multipart},
    body::Body,
    response::IntoResponse,
    http::{HeaderMap, header, StatusCode},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::{Duration, Instant};
use std::env;
use tower_http::cors::CorsLayer;
use plex::{PlexClient, PlexMovie, PlexShow};
use tmdb::TmdbClient;

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
            cache_duration: Duration::from_secs(300),
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

// ==================================================================================
// HANDLERS - WEBHOOK
// ==================================================================================

async fn handle_plex_webhook(
    Extension(state): Extension<Arc<AppState>>,
    mut multipart: Multipart,
) {
    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name().unwrap_or("") == "payload" {
            if let Ok(text) = field.text().await {
                if let Ok(payload) = serde_json::from_str::<PlexWebhookPayload>(&text) {
                    if payload.event == "library.new" {
                        if let Some(meta) = payload.metadata {
                            if meta.media_type == "movie" {
                                println!("🔔 Webhook : Nouveau film détecté (ID: {})", meta.rating_key);
                                let state_clone = state.clone();
                                tokio::spawn(async move {
                                    process_single_movie_by_id(state_clone, meta.rating_key).await;
                                });
                            } else if meta.media_type == "show" {
                                println!("🔔 Webhook : Nouvelle série détectée (ID: {})", meta.rating_key);
                                let state_clone = state.clone();
                                tokio::spawn(async move {
                                    process_single_show_by_id(state_clone, meta.rating_key).await;
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
            if let Ok(_) = processor::process_movie(&plex, &tmdb, movie).await {
                println!("🔄 Invalidation du cache suite au traitement...");
                let mut cache = state.library_cache.lock().await;
                cache.invalidate();
            }
        },
        Err(e) => println!("❌ Erreur Webhook (Détails film) : {:?}", e),
    }
}

async fn process_single_show_by_id(state: Arc<AppState>, rating_key: String) {
    let config = state.config.lock().await;
    let plex = PlexClient::new(config.plex_url.clone(), config.plex_token.clone());
    let tmdb = TmdbClient::new(config.tmdb_key.clone());
    drop(config);

    println!("⏳ Attente de 10s pour l'analyse Plex...");
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;

    match plex.get_show_details(&rating_key).await {
        Ok(show) => {
            if let Ok(_) = processor::process_show(&plex, &tmdb, show).await {
                println!("✅ Série traitée avec succès");
            }
        },
        Err(e) => println!("❌ Erreur Webhook (Détails série) : {:?}", e),
    }
}

// ==================================================================================
// HANDLERS - SCAN MANUEL FILMS
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
                        if movie.has_label("Rustizarr") {
                            println!("   ⏭️  SKIP : Film déjà traité (Label 'Rustizarr' trouvé).");
                            continue;
                        }

                        println!("   ✨ Nouveau film détecté, lancement du traitement...");
                        
                        if let Ok(msg) = processor::process_movie(&plex, &tmdb, movie).await {
                            report.push_str(&msg);
                        }
                    },
                    Err(e) => println!("   ⚠️ Erreur récupération détails: {:?}, passage au suivant.", e),
                };
            }
            
            println!("🔄 Scan terminé, invalidation du cache...");
            let mut cache = state.library_cache.lock().await;
            cache.invalidate();
            
            Json(report)
        }
        Err(e) => Json(format!("Erreur Plex: {:?}", e))
    }
}

// ==================================================================================
// HANDLERS - SCAN MANUEL SÉRIES
// ==================================================================================

async fn run_full_shows_scan(Extension(state): Extension<Arc<AppState>>) -> Json<String> {
    let config = state.config.lock().await;
    let plex = PlexClient::new(config.plex_url.clone(), config.plex_token.clone());
    let tmdb = TmdbClient::new(config.tmdb_key.clone());
    let shows_library_id = env::var("SHOWS_LIBRARY_ID").unwrap_or("2".to_string());
    drop(config);

    println!("🔍 Scan des séries...");

    match plex.get_shows_library_items(&shows_library_id).await {
        Ok(shows) => {
            let total = shows.len();
            println!("🔍 {} séries trouvées.", total);
            let mut report = String::new();

            for (index, show) in shows.iter().enumerate() {
                println!("---------------------------------------------------");
                println!("🔎 Analyse ({}/{}) : {}", index + 1, total, show.title);

                if show.has_label("Rustizarr") {
                    println!("   ⏭️  SKIP : Série déjà traitée");
                    continue;
                }

                println!("   ✨ Nouvelle série détectée, traitement...");
                
                if let Ok(msg) = processor::process_show(&plex, &tmdb, show.clone()).await {
                    report.push_str(&msg);
                    report.push('\n');
                }
            }
            
            Json(report)
        }
        Err(e) => Json(format!("Erreur Plex: {:?}", e))
    }
}

// ==================================================================================
// HANDLERS - API FILMS
// ==================================================================================

async fn get_library_json(Extension(state): Extension<Arc<AppState>>) -> Json<Vec<PlexMovie>> {
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

    println!("🔄 Cache MISS : Rechargement des données...");

    let config = state.config.lock().await;
    let plex = PlexClient::new(config.plex_url.clone(), config.plex_token.clone());
    let library_id = config.library_id.clone();
    drop(config);

    match plex.get_library_items_with_labels(&library_id).await {
        Ok(movies) => {
            let count_processed = movies.iter()
                .filter(|m| m.has_label("Rustizarr"))
                .count();
            
            println!("✅ Données chargées : {} films (dont {} traités)", 
                movies.len(), count_processed);
            
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

async fn refresh_library_cache(Extension(state): Extension<Arc<AppState>>) -> Json<serde_json::Value> {
    println!("🔄 Rafraîchissement manuel du cache demandé...");
    
    {
        let mut cache = state.library_cache.lock().await;
        cache.invalidate();
    }
    
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

// ==================================================================================
// HANDLERS - API SÉRIES
// ==================================================================================

async fn get_shows_json(Extension(state): Extension<Arc<AppState>>) -> Json<Vec<PlexShow>> {
    let config = state.config.lock().await;
    let plex = PlexClient::new(config.plex_url.clone(), config.plex_token.clone());
    let shows_library_id = env::var("SHOWS_LIBRARY_ID").unwrap_or("2".to_string());
    drop(config);

    match plex.get_shows_library_items(&shows_library_id).await {
        Ok(shows) => {
            let count_processed = shows.iter()
                .filter(|s| s.has_label("Rustizarr"))
                .count();
            
            println!("✅ Séries chargées : {} séries (dont {} traitées)", 
                shows.len(), count_processed);
            
            Json(shows)
        },
        Err(e) => {
            println!("❌ Erreur récupération séries : {:?}", e);
            Json(vec![])
        }
    }
}

async fn refresh_shows_cache(Extension(state): Extension<Arc<AppState>>) -> Json<serde_json::Value> {
    println!("🔄 Rafraîchissement séries demandé...");
    
    let config = state.config.lock().await;
    let plex = PlexClient::new(config.plex_url.clone(), config.plex_token.clone());
    let shows_library_id = env::var("SHOWS_LIBRARY_ID").unwrap_or("2".to_string());
    drop(config);

    match plex.get_shows_library_items(&shows_library_id).await {
        Ok(shows) => {
            let count_processed = shows.iter()
                .filter(|s| s.has_label("Rustizarr"))
                .count();
            
            Json(serde_json::json!({
                "success": true,
                "total": shows.len(),
                "processed": count_processed,
                "message": "Séries rafraîchies avec succès"
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

// ==================================================================================
// HANDLERS - IMAGES
// ==================================================================================

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

// ==================================================================================
// MAIN
// ==================================================================================

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

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
        .route("/scan", get(run_full_library_scan))
        .route("/webhook", post(handle_plex_webhook))
        .route("/api/library", get(get_library_json))
        .route("/api/library/refresh", post(refresh_library_cache)) 
        .route("/api/image/:id", get(get_plex_image))
        .route("/api/shows", get(get_shows_json))
        .route("/api/shows/refresh", post(refresh_shows_cache))
        .route("/scan-shows", get(run_full_shows_scan))
        .layer(CorsLayer::permissive())
        .layer(Extension(app_state));

    let port = env::var("PORT").unwrap_or("3000".to_string());
    let addr = format!("0.0.0.0:{}", port);

    println!("🚀 Serveur lancé sur http://{}", addr);
    println!("👉 Endpoint Webhook : http://TON_IP_LOCALE:{}/webhook", port);
    if let Ok(cwd) = std::env::current_dir() { 
        println!("📂 Dossier d'exécution (CWD) : {:?}", cwd); 
    }

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
