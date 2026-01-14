use serde::{Deserialize, Serialize};
use anyhow::Result;

// --- Structures ---

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
    
    // Modification pour gérer le bug Array/Object de Plex
    #[serde(rename = "Label")]
    pub labels: Option<serde_json::Value>,
}

// --- NOUVEAU BLOC : Méthodes liées au Film ---
impl PlexMovie {
    /// Vérifie si le film contient un label spécifique (insensible à la casse)
    pub fn has_label(&self, tag_to_find: &str) -> bool {
        let target = tag_to_find.to_lowercase();

        if let Some(value) = &self.labels {
            // Cas 1 : C'est un tableau (plusieurs labels)
            if let Some(arr) = value.as_array() {
                return arr.iter().any(|obj| {
                    obj.get("tag")
                       .and_then(|v| v.as_str())
                       .map(|s| s.to_lowercase() == target)
                       .unwrap_or(false)
                });
            }
            // Cas 2 : C'est un objet unique (un seul label) -> Le piège de Plex !
            if let Some(obj) = value.as_object() {
                 return obj.get("tag")
                       .and_then(|v| v.as_str())
                       .map(|s| s.to_lowercase() == target)
                       .unwrap_or(false);
            }
        }
        false
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PlexLabel {
    pub tag: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PlexGuid { pub id: String }

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
    metadata: Option<Vec<PlexMovie>>,
}

// --- Logique Métier (Client API) ---

pub struct PlexClient {
    client: reqwest::Client,
    base_url: String,
    token: String,
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

        let content = response.text().await?;
        
        match serde_json::from_str::<PlexResponse>(&content) {
            Ok(plex_resp) => Ok(plex_resp.media_container.metadata.unwrap_or_default()),
            Err(e) => {
                println!("❌ ECHEC PARSING: {:?}", e);
                Err(anyhow::anyhow!("Erreur parsing: {:?}", e))
            }
        }
    }

    // NOUVELLE fonction pour charger les détails (avec labels) de plusieurs films
    pub async fn get_library_items_with_labels(&self, library_id: &str) -> Result<Vec<PlexMovie>> {
        // 1. Récupère la liste rapide (sans labels)
        let movies = self.get_library_items(library_id).await?;
        let total = movies.len();
        
        println!("📚 Chargement des labels pour {} films...", total);
        
        // 2. Charge les détails (avec labels) pour chaque film
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
                    detailed_movies.push(movie); // Garde l'original sans labels
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

        let content = response.text().await?;
        
        let plex_resp: PlexResponse = serde_json::from_str(&content)?;
        
        if let Some(metadata) = plex_resp.media_container.metadata {
            if let Some(movie) = metadata.into_iter().next() {
                return Ok(movie);
            }
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

    pub async fn upload_poster(&self, rating_key: &str, image_data: Vec<u8>) -> Result<()> {
        let url = format!("{}/library/metadata/{}/posters", self.base_url, rating_key);
        
        println!("📤 Upload vers Plex (Raw Body)... Taille: {} octets", image_data.len());

        let response = self.client
            .post(&url)
            .header("X-Plex-Token", &self.token)
            .header("Content-Type", "image/jpeg") 
            .header("Accept", "application/json")
            .body(image_data) 
            .send()
            .await?;

        if response.status().is_success() {
            println!("✅ Plex a accepté l'image (Code 200/201)");
            Ok(())
        } else {
            let status = response.status();
            Err(anyhow::anyhow!("Echec upload Plex: {}", status))
        }
    }
} // <-- Cette accolade ferme impl PlexClient