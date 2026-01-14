
use serde::{Deserialize, Serialize};
use anyhow::Result;

// --- Structures ---

#[derive(Clone)]
pub struct PlexClient {
    client: reqwest::Client,
    base_url: String,
    token: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PlexMovie {
    pub title: String,
    #[serde(rename = "ratingKey")]
    pub rating_key: String,
    #[serde(rename = "audienceRating")]
    pub audience_rating: Option<f64>,
    #[serde(rename = "Guid")]
    pub guids: Option<Vec<PlexGuid>>, 
    #[serde(rename = "guid")]
    pub guid_str: Option<String>,
    #[serde(rename = "year")]
    pub year: Option<u16>,
    #[serde(rename = "Media")]
    pub media: Option<Vec<PlexMedia>>,
    #[serde(rename = "addedAt")]
    pub added_at: Option<u64>,
    
    #[serde(rename = "Label")]
    pub labels: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PlexShow {
    pub title: String,
    #[serde(rename = "ratingKey")]
    pub rating_key: String,
    pub year: Option<u32>,
    #[serde(rename = "audienceRating")]
    pub audience_rating: Option<f64>,
    #[serde(rename = "addedAt")]
    pub added_at: Option<u64>,
    
    #[serde(rename = "Guid")]
    pub guid: Option<Vec<PlexGuid>>,
    
    #[serde(rename = "Label")]
    pub label: Option<serde_json::Value>,  // ← Utilise Value comme pour les films
    
    #[serde(rename = "Media")]
    pub media: Option<Vec<PlexMedia>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PlexSeason {
    pub title: String,
    #[serde(rename = "ratingKey")]
    pub rating_key: String,
    #[serde(rename = "index")]
    pub season_number: u32,
    #[serde(rename = "parentTitle")]
    pub show_title: String,
    #[serde(rename = "parentRatingKey")]
    pub show_rating_key: String,
    #[serde(rename = "audienceRating")]
    pub audience_rating: Option<f64>,
    #[serde(rename = "addedAt")]
    pub added_at: Option<u64>,
    #[serde(rename = "Media")]
    pub media: Option<Vec<PlexMedia>>,
    #[serde(rename = "Label")]
    pub label: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PlexLabel {
    pub tag: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PlexGuid { 
    pub id: String 
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PlexMedia {
    #[serde(rename = "videoResolution")]
    pub video_resolution: Option<String>, 
    #[serde(rename = "audioCodec")]
    pub audio_codec: Option<String>,      
    
    #[serde(rename = "Part")]
    pub parts: Option<serde_json::Value>,     
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PlexPart {
    #[serde(rename = "Stream")]
    pub streams: Option<serde_json::Value>, 
}

#[derive(Debug, Deserialize)]
struct PlexResponse {
    #[serde(rename = "MediaContainer")]
    media_container: MediaContainer,
}

#[derive(Debug, Deserialize)]
struct MediaContainer {
    #[serde(rename = "Metadata")]
    metadata: Option<Vec<serde_json::Value>>,
}

// --- Implémentations ---

impl PlexMovie {
    /// Vérifie si le film contient un label spécifique (insensible à la casse)
    pub fn has_label(&self, tag_to_find: &str) -> bool {
        let target = tag_to_find.to_lowercase();

        if let Some(value) = &self.labels {
            if let Some(arr) = value.as_array() {
                return arr.iter().any(|obj| {
                    obj.get("tag")
                       .and_then(|v| v.as_str())
                       .map(|s| s.to_lowercase() == target)
                       .unwrap_or(false)
                });
            }
            if let Some(obj) = value.as_object() {
                 return obj.get("tag")
                       .and_then(|v| v.as_str())
                       .map(|s| s.to_lowercase() == target)
                       .unwrap_or(false);
            }
        }
        false
    }
    
    pub fn is_recently_added(&self) -> bool {
        if let Some(added_at) = self.added_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let days_ago = (now - added_at) / 86400;
            days_ago <= 7
        } else {
            false
        }
    }
}

impl PlexShow {
    /// Vérifie si la série contient un label spécifique (insensible à la casse)
    pub fn has_label(&self, tag_to_find: &str) -> bool {
        let target = tag_to_find.to_lowercase();

        if let Some(value) = &self.label {
            if let Some(arr) = value.as_array() {
                return arr.iter().any(|obj| {
                    obj.get("tag")
                       .and_then(|v| v.as_str())
                       .map(|s| s.to_lowercase() == target)
                       .unwrap_or(false)
                });
            }
            if let Some(obj) = value.as_object() {
                 return obj.get("tag")
                       .and_then(|v| v.as_str())
                       .map(|s| s.to_lowercase() == target)
                       .unwrap_or(false);
            }
        }
        false
    }
    
    pub fn is_recently_added(&self) -> bool {
        if let Some(added_at) = self.added_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let days_ago = (now - added_at) / 86400;
            days_ago <= 30
        } else {
            false
        }
    }
}

impl PlexSeason {
    pub fn has_label(&self, label_name: &str) -> bool {
        let target = label_name.to_lowercase();
        
        if let Some(value) = &self.label {
            if let Some(arr) = value.as_array() {
                return arr.iter().any(|obj| {
                    obj.get("tag")
                       .and_then(|v| v.as_str())
                       .map(|s| s.to_lowercase() == target)
                       .unwrap_or(false)
                });
            }
            if let Some(obj) = value.as_object() {
                 return obj.get("tag")
                       .and_then(|v| v.as_str())
                       .map(|s| s.to_lowercase() == target)
                       .unwrap_or(false);
            }
        }
        false
    }
    
    pub fn is_recently_added(&self) -> bool {
        if let Some(added_at) = self.added_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let days_ago = (now - added_at) / 86400;
            days_ago <= 30
        } else {
            false
        }
    }
}

impl PlexClient {
    pub fn new(base_url: String, token: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();
        Self { client, base_url, token }
    }

    pub async fn add_label(&self, rating_key: &str, label: &str) -> Result<()> {
        let url = format!("{}/library/metadata/{}?label%5B0%5D.tag.tag={}", self.base_url, rating_key, label);
        
        let response = self.client
            .put(&url)
            .header("X-Plex-Token", &self.token)
            .header("Accept", "application/json")
            .send()
            .await?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Echec ajout Label: {}", response.status()))
        }
    }

    pub async fn get_labels(&self, rating_key: &str) -> Result<Vec<String>> {
        let url = format!(
            "{}/library/metadata/{}",
            self.base_url, rating_key
        );
        
        let resp = self.client
            .get(&url)
            .header("X-Plex-Token", &self.token)
            .header("Accept", "application/json")
            .send()
            .await?;
        
        if !resp.status().is_success() {
            return Ok(Vec::new());
        }
        
        let json: serde_json::Value = resp.json().await?;
        let mut labels = Vec::new();
        
        if let Some(metadata) = json["MediaContainer"]["Metadata"].get(0) {
            if let Some(label_value) = metadata.get("Label") {
                if let Some(label_array) = label_value.as_array() {
                    for label in label_array {
                        if let Some(tag) = label["tag"].as_str() {
                            labels.push(tag.to_string());
                        }
                    }
                } else if let Some(label_obj) = label_value.as_object() {
                    if let Some(tag) = label_obj.get("tag").and_then(|v| v.as_str()) {
                        labels.push(tag.to_string());
                    }
                }
            }
        }
        
        Ok(labels)
    }

    // ========== FILMS ==========

    pub async fn get_library_items(&self, library_id: &str) -> Result<Vec<PlexMovie>> {
        let url = format!(
            "{}/library/sections/{}/all?type=1&includeGuids=1",
            self.base_url, 
            library_id
        );
        
        let response = self.client
            .get(&url)
            .header("Accept", "application/json")
            .header("X-Plex-Token", &self.token)
            .send()
            .await?;
        
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Erreur Plex HTTP {}", response.status()));
        }

        let json: serde_json::Value = response.json().await?;
        let mut movies = Vec::new();
        
        if let Some(metadata) = json["MediaContainer"]["Metadata"].as_array() {
            for item in metadata {
                if let Ok(movie) = serde_json::from_value(item.clone()) {
                    movies.push(movie);
                }
            }
        }
        
        Ok(movies)
    }

    pub async fn get_library_items_with_labels(&self, library_id: &str) -> Result<Vec<PlexMovie>> {
        let movies = self.get_library_items(library_id).await?;
        let total = movies.len();
        
        println!("📚 Chargement des labels pour {} films...", total);
        
        let mut detailed_movies = Vec::new();
        
        for (i, movie) in movies.into_iter().enumerate() {
            match self.get_item_details(&movie.rating_key).await {
                Ok(detailed) => {
                    if (i + 1) % 20 == 0 {
                        println!("   ⏳ Progression: {}/{}", i + 1, total);
                    }
                    detailed_movies.push(detailed);
                },
                Err(e) => {
                    println!("   ⚠️  Erreur détails pour {}: {:?}", movie.title, e);
                    detailed_movies.push(movie);
                }
            }
        }
        
        println!("✅ Labels chargés pour {} films", detailed_movies.len());
        Ok(detailed_movies)
    }

    pub async fn get_item_details(&self, rating_key: &str) -> Result<PlexMovie> {
        let url = format!("{}/library/metadata/{}", self.base_url, rating_key);

        let response = self.client
            .get(&url)
            .header("Accept", "application/json")
            .header("X-Plex-Token", &self.token)
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;
        
        if let Some(metadata) = json["MediaContainer"]["Metadata"].get(0) {
            let movie: PlexMovie = serde_json::from_value(metadata.clone())?;
            return Ok(movie);
        }
        
        Err(anyhow::anyhow!("Film introuvable"))
    }

    pub fn extract_tmdb_id(movie: &PlexMovie) -> Option<String> {
        if let Some(guids) = &movie.guids {
            for guid in guids {
                if guid.id.starts_with("tmdb://") { 
                    return Some(guid.id.replace("tmdb://", "")); 
                }
            }
        }
        if let Some(guid_str) = &movie.guid_str {
            if guid_str.contains("themoviedb://") {
                let parts: Vec<&str> = guid_str.split("themoviedb://").collect();
                if parts.len() > 1 { 
                    return Some(parts[1].split('?').next().unwrap_or("").to_string()); 
                }
            }
        }
        None
    }

    // ========== SÉRIES ==========

    /// Récupère la liste des séries d'une bibliothèque (avec JSON API comme les films)
    pub async fn get_shows_library_items(&self, library_id: &str) -> Result<Vec<PlexShow>> {
        let url = format!(
            "{}/library/sections/{}/all?type=2&includeGuids=1",
            self.base_url, 
            library_id
        );
        
        println!("🔗 Récupération séries : library_id={}", library_id);
        
        let response = self.client
            .get(&url)
            .header("Accept", "application/json")
            .header("X-Plex-Token", &self.token)
            .send()
            .await?;
        
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Erreur Plex HTTP {}", response.status()));
        }

        let json: serde_json::Value = response.json().await?;
        let mut shows: Vec<PlexShow> = Vec::new();
        
        if let Some(metadata) = json["MediaContainer"]["Metadata"].as_array() {
            for item in metadata {
                if let Ok(show) = serde_json::from_value(item.clone()) {
                    shows.push(show);
                }
            }
        }
        
        println!("✅ {} séries parsées", shows.len());
        
        // Debug première série
        if let Some(first) = shows.first() {
            println!("🔍 Exemple : {} - Has Label: {}", 
                first.title,
                first.has_label("Rustizarr")
            );
        }
        
        Ok(shows)
    }

    /// Récupère les détails complets d'une série
    pub async fn get_show_details(&self, rating_key: &str) -> Result<PlexShow> {
        let url = format!("{}/library/metadata/{}", self.base_url, rating_key);

        let response = self.client
            .get(&url)
            .header("Accept", "application/json")
            .header("X-Plex-Token", &self.token)
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;
        
        if let Some(metadata) = json["MediaContainer"]["Metadata"].get(0) {
            let show: PlexShow = serde_json::from_value(metadata.clone())?;
            return Ok(show);
        }
        
        Err(anyhow::anyhow!("Série introuvable"))
    }

    /// Extrait l'ID TMDB d'une série
    pub fn extract_tmdb_id_from_show(show: &PlexShow) -> Option<String> {
        if let Some(guids) = &show.guid {
            for guid in guids {
                if guid.id.starts_with("tmdb://") {
                    return Some(guid.id.replace("tmdb://", ""));
                }
            }
        }
        None
    }

    // ========== SAISONS ==========

    pub async fn get_show_seasons(&self, show_rating_key: &str) -> Result<Vec<PlexSeason>> {
        let url = format!(
            "{}/library/metadata/{}/children",
            self.base_url, show_rating_key
        );
        
        let resp = self.client
            .get(&url)
            .header("X-Plex-Token", &self.token)
            .header("Accept", "application/json")
            .send()
            .await?;
        
        let json: serde_json::Value = resp.json().await?;
        let mut seasons = Vec::new();
        
        if let Some(metadata) = json["MediaContainer"]["Metadata"].as_array() {
            for item in metadata {
                if let Ok(season) = serde_json::from_value(item.clone()) {
                    seasons.push(season);
                }
            }
        }
        
        Ok(seasons)
    }

    // ========== COMMUN ==========

    pub async fn upload_poster(&self, rating_key: &str, image_data: Vec<u8>) -> Result<()> {
        let url = format!("{}/library/metadata/{}/posters", self.base_url, rating_key);

        let response = self.client
            .post(&url)
            .header("X-Plex-Token", &self.token)
            .header("Content-Type", "image/jpeg") 
            .header("Accept", "application/json")
            .body(image_data) 
            .send()
            .await?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            Err(anyhow::anyhow!("Echec upload Plex: {}", status))
        }
    }
}
