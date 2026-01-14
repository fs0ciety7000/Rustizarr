// backend/src/processor.rs
use crate::plex::{PlexClient, PlexMovie, PlexMedia, PlexShow, PlexSeason};
use crate::tmdb::TmdbClient;
use crate::image_ops::ImageProcessor;
use anyhow::Result;
use std::path::Path;
use std::io::Cursor;
use std::env;
use futures::stream::{self, StreamExt};

// ==================== FILMS ====================

/// Fonction principale de traitement d'un film
pub async fn process_movie(
    plex: &PlexClient,
    tmdb: &TmdbClient,
    movie: PlexMovie
) -> Result<String> {
    
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
            
            match ImageProcessor::download_image(&url).await {
                Ok(mut poster) => {
                    println!("   ✅ Image téléchargée : {}x{}", poster.width(), poster.height());
                    
                    let overlays_base = get_overlays_path();
                    
                    // Effets de base
                    poster = ImageProcessor::add_gradient_masks(poster, &overlays_base)?;
                    println!("   ✅ Gradients appliqués");
                    
                    poster = ImageProcessor::add_movie_title(poster, &movie.title, &overlays_base)?;
                    println!("   ✅ Titre ajouté");

                    let base_path = Path::new(&overlays_base).join("media_info");
                    let audience_path = Path::new(&overlays_base).join("audience_score");
                    
                    let mut top_left_index = 0;

                    // Overlay RÉSOLUTION (haut-gauche)
                    if let Some(media_list) = &movie.media {
                        if let Some(media) = media_list.first() {
                            if let Some(res_file) = get_resolution_filename(media) {
                                let path = base_path.join("resolution").join(res_file);
                                if let Ok(img) = ImageProcessor::add_overlay(poster.clone(), &path, top_left_index, false, 0.065) {
                                    poster = img;
                                    top_left_index += 1;
                                    println!("   ✅ Overlay résolution ajouté");
                                }
                            }
                        }
                    }

                    // Overlay ÉDITION (haut-gauche)
                    if let Some(edition_file) = get_edition_filename(&movie) {
                        let path = base_path.join("edition").join(edition_file);
                        if let Ok(img) = ImageProcessor::add_overlay(poster.clone(), &path, top_left_index, false, 0.065) {
                            poster = img;
                            println!("   ✅ Overlay édition ajouté");
                        }
                    }

                    // Overlay CODEC AUDIO (bas-gauche)
                    if let Some(media_list) = &movie.media {
                        if let Some(media) = media_list.first() {
                            if let Some(audio_file) = get_codec_combo_filename(media) {
                                let path = base_path.join("codec").join(audio_file);
                                if let Ok(img) = ImageProcessor::add_overlay(poster.clone(), &path, 0, true, 0.050) {
                                    poster = img;
                                    println!("   ✅ Overlay codec ajouté");
                                }
                            }
                        }
                    }

                    // Overlay AUDIENCE SCORE (bas-droite)
                    if let Some(rating) = movie.audience_rating {
                        println!("   🎯 Score audience détecté : {}/10", rating);
                        let badge_file = get_audience_badge_filename(rating);
                        let full_path = audience_path.join(badge_file);
                        
                        if let Ok(img) = ImageProcessor::add_overlay_bottom_right(poster.clone(), &full_path, 0.065, Some(rating), &overlays_base) {
                            poster = img;
                            println!("   ✅ Badge audience ajouté avec note {:.1}", rating);
                        }
                    }

                    // ✅ BORDURE : Recently Added OU Inner Glow
                    if movie.is_recently_added() {
                        poster = ImageProcessor::add_status_border(poster, &overlays_base, "recently_added.png")?;
                        println!("   ✅ Bordure 'Recently Added' appliquée");
                    } else {
                        poster = ImageProcessor::add_inner_glow_border(poster, &overlays_base)?;
                        println!("   ✅ Inner glow appliqué");
                    }

                    // Upload vers Plex
                    let rgb_poster = poster.to_rgb8(); 
                    let mut bytes: Vec<u8> = Vec::new();
                    rgb_poster.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Jpeg)?;

                    match plex.upload_poster(&movie.rating_key, bytes).await {
                        Ok(_) => {
                            let msg = format!("✅ SUCCÈS : '{}'", movie.title);
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
                },
                Err(e) => {
                    println!("   ❌ ERREUR TÉLÉCHARGEMENT : {:?}", e);
                    return Ok("Échec téléchargement image".to_string());
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

// ==================== SÉRIES ====================

/// Fonction principale de traitement d'une série
pub async fn process_show(
    plex: &PlexClient,
    tmdb: &TmdbClient,
    show: PlexShow
) -> Result<String> {
    
    let tmdb_id_opt = PlexClient::extract_tmdb_id_from_show(&show);

    if let Some(tmdb_id) = tmdb_id_opt {
        let mut final_url = None;
        
        // Récupération du poster
        match tmdb.get_show_textless_poster(&tmdb_id).await {
            Ok(Some(url)) => final_url = Some(url),
            Ok(None) => {
                println!("   ⚠️ Pas de poster textless. Tentative poster standard...");
                if let Ok(Some(std_url)) = tmdb.get_show_standard_poster(&tmdb_id).await {
                    final_url = Some(std_url);
                }
            }
            Err(e) => println!("   ❌ Erreur API TMDB : {:?}", e),
        }

        // Récupération du status
        let show_status = tmdb.get_show_status(&tmdb_id).await.ok().flatten();

        if let Some(url) = final_url {
            println!("   📸 Poster trouvé, téléchargement...");
            
            match ImageProcessor::download_image(&url).await {
                Ok(mut poster) => {
                    println!("   ✅ Image téléchargée : {}x{}", poster.width(), poster.height());
                    
                    let overlays_base = get_overlays_path();
                    
                    // Effets de base
                    poster = ImageProcessor::add_gradient_masks(poster, &overlays_base)?;
                    println!("   ✅ Gradients appliqués");

                    poster = ImageProcessor::add_movie_title(poster, &show.title, &overlays_base)?;
                    println!("   ✅ Titre ajouté");

                    let audience_path = Path::new(&overlays_base).join("audience_score");

                    // ❌ PAS d'overlay résolution pour les séries
                    // ❌ PAS d'overlay codec pour les séries

                    // Overlay AUDIENCE SCORE (bas-droite)
                    if let Some(rating) = show.audience_rating {
                        println!("   🎯 Score audience détecté : {}/10", rating);
                        let badge_file = get_audience_badge_filename(rating);
                        let full_path = audience_path.join(badge_file);
                        
                        if let Ok(img) = ImageProcessor::add_overlay_bottom_right(poster.clone(), &full_path, 0.065, Some(rating), &overlays_base) {
                            poster = img;
                            println!("   ✅ Badge audience ajouté avec note {:.1}", rating);
                        }
                    }

                    // ✅ BORDURE : Status > Recently Added > Inner Glow
                    if let Some(ref status) = show_status {
                        println!("   🔍 Status de la série : '{}'", status);
                        let status_file = get_status_filename(status);
                        println!("   📂 Fichier status : {}", status_file);
                        poster = ImageProcessor::add_status_border(poster, &overlays_base, status_file)?;
                    } else if show.is_recently_added() {
                        println!("   📅 Série récente (< 15j), bordure 'Recently Added'");
                        poster = ImageProcessor::add_status_border(poster, &overlays_base, "recently_added.png")?;
                    } else {
                        println!("   ✅ Pas de status ni récent, application inner glow");
                        poster = ImageProcessor::add_inner_glow_border(poster, &overlays_base)?;
                    }

                    // Upload vers Plex
                    let rgb_poster = poster.to_rgb8(); 
                    let mut bytes: Vec<u8> = Vec::new();
                    rgb_poster.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Jpeg)?;

                    plex.upload_poster(&show.rating_key, bytes).await?;
                    plex.add_label(&show.rating_key, "Rustizarr").await.ok();
                    
                    let msg = format!("✅ SUCCÈS : '{}'", show.title);
                    println!("{}", msg);
                    return Ok(msg);
                },
                Err(e) => {
                    println!("   ❌ ERREUR TÉLÉCHARGEMENT : {:?}", e);
                    return Ok("Échec téléchargement image".to_string());
                }
            }
        } else {
            println!("   ❌ ABANDON : Aucune image trouvée sur TMDB.");
        }
    } else {
        println!("   ⚠️ Pas d'ID TMDB trouvé.");
    }
    
    Ok("Série ignorée ou échec partiel".to_string())
}

// ==================== SAISONS ====================

/// Traite le poster d'une saison
pub async fn process_season(
    plex: &PlexClient,
    tmdb: &TmdbClient,
    season: PlexSeason,
    show_tmdb_id: &str,
    show_status: Option<String>
) -> Result<String> {
    
    let poster_url = tmdb.get_season_poster(show_tmdb_id, season.season_number).await?;
    
    if let Some(url) = poster_url {
        println!("   📸 Poster saison {} trouvé, téléchargement...", season.season_number);
        
        match ImageProcessor::download_image(&url).await {
            Ok(mut poster) => {
                println!("   ✅ Image téléchargée : {}x{}", poster.width(), poster.height());
                
                let overlays_base = get_overlays_path();
                
                // Effets de base
                poster = ImageProcessor::add_gradient_masks(poster, &overlays_base)?;
                println!("   ✅ Gradients appliqués");
                
                // Titre : "NOM SÉRIE - Saison X"
                let title_text = format!("{} - Saison {}", season.show_title, season.season_number);
                poster = ImageProcessor::add_movie_title(poster, &title_text, &overlays_base)?;
                println!("   ✅ Titre ajouté");

                let audience_path = Path::new(&overlays_base).join("audience_score");

                // ❌ PAS d'overlay résolution pour les saisons
                // ❌ PAS d'overlay codec pour les saisons

                // Audience Score
                if let Some(rating) = season.audience_rating {
                    println!("   🎯 Score audience saison : {}/10", rating);
                    let badge_file = get_audience_badge_filename(rating);
                    let full_path = audience_path.join(badge_file);
                    
                    if let Ok(img) = ImageProcessor::add_overlay_bottom_right(poster.clone(), &full_path, 0.065, Some(rating), &overlays_base) {
                        poster = img;
                        println!("   ✅ Badge audience ajouté");
                    }
                }

                // ✅ BORDURE : Status (du show) > Recently Added (de la saison) > Inner Glow
                if let Some(status) = show_status {
                    println!("   🔍 Status du show (pour saison) : '{}'", status);
                    let status_file = get_status_filename(&status);
                    poster = ImageProcessor::add_status_border(poster, &overlays_base, status_file)?;
                } else if season.is_recently_added() {
                    println!("   📅 Saison récente (< 15j), bordure 'Recently Added'");
                    poster = ImageProcessor::add_status_border(poster, &overlays_base, "recently_added.png")?;
                } else {
                    println!("   ✅ Pas de status ni récent, application inner glow");
                    poster = ImageProcessor::add_inner_glow_border(poster, &overlays_base)?;
                }

                // Upload vers Plex
                let rgb_poster = poster.to_rgb8(); 
                let mut bytes: Vec<u8> = Vec::new();
                rgb_poster.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Jpeg)?;

                plex.upload_poster(&season.rating_key, bytes).await?;
                plex.add_label(&season.rating_key, "Rustizarr").await.ok();
                
                Ok(format!("✅ Saison {} traitée", season.season_number))
            },
            Err(e) => Err(anyhow::anyhow!("Erreur téléchargement: {:?}", e))
        }
    } else {
        Ok("❌ Pas de poster trouvé".to_string())
    }
}

// ==================== PARALLÉLISATION ====================

pub async fn process_library_parallel(
    plex: &PlexClient,
    tmdb: &TmdbClient,
    movies: Vec<PlexMovie>,
    concurrency: usize,
    force: bool
) -> Vec<(String, anyhow::Result<String>)> {
    println!("🚀 Traitement parallèle : {} films, {} threads", movies.len(), concurrency);
    
    let results = stream::iter(movies)
        .map(|movie| {
            let plex_clone = plex.clone();
            let tmdb_clone = tmdb.clone();
            async move {
                let title = movie.title.clone();
                
                if !force && movie.has_label("Rustizarr") {
                    return (title.clone(), Ok("⏭️ Déjà traité".to_string()));
                }
                
                let result = process_movie(&plex_clone, &tmdb_clone, movie).await;
                (title, result)
            }
        })
        .buffer_unordered(concurrency)
        .collect::<Vec<_>>()
        .await;
    
    results
}

pub async fn process_shows_parallel(
    plex: &PlexClient,
    tmdb: &TmdbClient,
    shows: Vec<PlexShow>,
    concurrency: usize,
    force: bool
) -> Vec<(String, anyhow::Result<String>)> {
    println!("🚀 Traitement parallèle : {} séries, {} threads", shows.len(), concurrency);
    
    let results = stream::iter(shows)
        .map(|show| {
            let plex_clone = plex.clone();
            let tmdb_clone = tmdb.clone();
            async move {
                let title = show.title.clone();
                
                if !force && show.has_label("Rustizarr") {
                    return (title.clone(), Ok("⏭️ Déjà traité".to_string()));
                }
                
                let result = process_show(&plex_clone, &tmdb_clone, show).await;
                (title, result)
            }
        })
        .buffer_unordered(concurrency)
        .collect::<Vec<_>>()
        .await;
    
    results
}

// ==================== FONCTIONS HELPER ====================

fn get_overlays_path() -> String {
    env::var("OVERLAYS_PATH")
        .unwrap_or_else(|_| {
            if let Ok(cwd) = env::current_dir() {
                if cwd.ends_with("backend") {
                    cwd.parent().unwrap().join("overlays").to_string_lossy().to_string()
                } else {
                    cwd.join("overlays").to_string_lossy().to_string()
                }
            } else {
                "./overlays".to_string()
            }
        })
}

pub fn get_forced_tmdb_id(title: &str) -> Option<String> {
    match title.to_lowercase().as_str() {
        "abyss" => Some("1025527".to_string()), 
        "kingsman : le cercle d'or" | "kingsman the golden circle" => Some("343668".to_string()),
        _ => None
    }
}

pub fn get_edition_filename(movie: &PlexMovie) -> Option<&str> {
    let t = movie.title.to_lowercase();
    if t.contains("director's cut") || t.contains("director cut") { 
        Some("Directors-Cut.png") 
    } else if t.contains("extended") { 
        Some("Extended-Edition.png") 
    } else if t.contains("remastered") { 
        Some("Remastered.png") 
    } else if t.contains("uncut") { 
        Some("Uncut.png") 
    } else if t.contains("imax") { 
        Some("IMAX.png") 
    } else { 
        None 
    }
}

pub fn get_resolution_filename(media: &PlexMedia) -> Option<String> {
    let raw_res = media.video_resolution.as_deref().unwrap_or("").to_lowercase();
    match raw_res.as_str() {
        "4k" | "ultra hd" => Some("Ultra-HD.png".to_string()),
        "1080" | "1080p" | "fhd" => Some("1080P.png".to_string()), 
        _ => None, 
    }
}

pub fn get_audience_badge_filename(rating: f64) -> &'static str {
    if rating >= 8.0 { 
        "audience_score_high.png" 
    } else if rating >= 6.0 { 
        "audience_score_mid.png" 
    } else { 
        "audience_score_low.png" 
    }
}

pub fn get_status_filename(status: &str) -> &'static str {
    match status.to_lowercase().as_str() {
        "returning series" | "returning" => "returning_border.png",
        "canceled" | "cancelled" => "cancelled_full.png",
        "ended" => "ended_border.png",
        "in production" | "airing" => "airing_border.png",
        _ => "airing_border.png",
    }
}

pub fn get_codec_combo_filename(media: &PlexMedia) -> Option<String> {
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
        let parts_slice: &[serde_json::Value] = if let Some(arr) = parts_value.as_array() { 
            arr.as_slice() 
        } else { 
            std::slice::from_ref(parts_value) 
        };

        for part in parts_slice {
            let maybe_streams = part.get("Stream").or_else(|| part.get("stream"));
            if let Some(streams_value) = maybe_streams {
                has_streams_access = true;
                let streams_slice: &[serde_json::Value] = if let Some(arr) = streams_value.as_array() { 
                    arr.as_slice() 
                } else { 
                    std::slice::from_ref(streams_value) 
                };

                for stream in streams_slice {
                    let stream_type = stream.get("streamType").and_then(|v| v.as_u64()).unwrap_or(0);
                    
                    if stream_type == 1 { // VIDEO
                        let display = stream.get("displayTitle").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                        let title = stream.get("title").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                        if stream.get("doviprofile").is_some() || stream.get("DOVIProfile").is_some() || stream.get("DOVIPresent").is_some() { 
                            is_dv = true; 
                        }
                        if display.contains("dolby vision") || title.contains("dolby vision") || display.contains("dovi") || title.contains("dovi") { 
                            is_dv = true; 
                        }
                        if display.contains("hdr10+") || title.contains("hdr10+") { 
                            is_plus = true; 
                        } else if display.contains("hdr") || title.contains("hdr") { 
                            is_hdr = true; 
                        }
                    }

                    if stream_type == 2 { // AUDIO
                        let display = stream.get("displayTitle").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                        let title = stream.get("title").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                        let codec = stream.get("codec").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                        let profile = stream.get("audioProfile").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                        found_audio_codec = codec.clone();

                        if title.contains("atmos") || display.contains("atmos") { 
                            has_atmos = true; 
                        }
                        match codec.as_str() {
                            "truehd" => has_truehd = true,
                            "dca" | "dts" => {
                                if profile == "dts:x" { 
                                    has_dts_x = true; 
                                }
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
        if is_dv && is_hdr { 
            Some("DV-HDR") 
        } else if is_dv && is_plus { 
            Some("DV-Plus") 
        } else if is_dv { 
            Some("DV") 
        } else if is_plus { 
            Some("Plus") 
        } else if is_hdr { 
            Some("HDR") 
        } else { 
            None 
        }
    } else { 
        None 
    };

    let audio_part = if has_streams_access {
        if has_truehd && has_atmos { 
            Some("TrueHD-Atmos") 
        } else if has_truehd { 
            Some("TrueHD") 
        } else if has_dts_x { 
            Some("DTS-X") 
        } else if has_dts_hd { 
            Some("DTS-HD") 
        } else if has_atmos { 
            Some("Atmos") 
        } else if has_dd_plus { 
            Some("DigitalPlus") 
        } else { 
            None 
        }
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
